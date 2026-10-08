//! Sub-Agent 运行时：统一子 Agent 生命周期管理。
//!
//! 职责：
//! - 通过 TaskRuntime 提交子 Agent 任务
//! - 子 Agent 生命周期：spawn → running → completed/failed/cancelled
//! - 结果聚合（多个子 Agent 的输出合并）
//!
//! 设计：SubAgentRuntime 不直接执行 LLM 调用，而是将子 Agent 任务
//! 提交给 TaskRuntime，由 TaskRuntime 负责排队、重试、超时。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::{
    application::event_bus::EventBus,
    domain::task::{TaskKind, TaskPriority, TaskStatus},
    ports::task_runtime::{TaskError, TaskHandle, TaskRequest, TaskRuntime},
};

/// 子 Agent 任务描述。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubAgentRequest {
    /// 子 Agent 名称（日志 + 调试用）。
    pub name: String,
    /// 子 Agent 类型（决定 TaskKind 映射）。
    pub agent_type: SubAgentType,
    /// 子 Agent 的提示词 / 任务描述。
    pub prompt: String,
    /// 优先级。
    pub priority: TaskPriority,
    /// 关联的工作区 ID。
    pub workspace_id: String,
    /// 关联的会话 ID（可选）。
    pub conversation_id: Option<String>,
    /// 附加参数。
    pub parameters: HashMap<String, serde_json::Value>,
    /// 最大重试次数。
    pub max_retries: u32,
    /// 超时秒数。
    pub timeout_secs: Option<u64>,
}

/// 子 Agent 类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubAgentType {
    /// 图片生成 Agent。
    ImageGenerator,
    /// 视频生成 Agent。
    VideoGenerator,
    /// 文本分析 Agent。
    Analyzer,
    /// 通用 Agent。
    Generic,
}

impl SubAgentType {
    /// 映射到 TaskKind。
    fn to_task_kind(&self) -> TaskKind {
        match self {
            SubAgentType::ImageGenerator => TaskKind::ImageGeneration,
            SubAgentType::VideoGenerator => TaskKind::VideoGeneration,
            SubAgentType::Analyzer => TaskKind::Analysis,
            SubAgentType::Generic => TaskKind::Generic,
        }
    }
}

/// 子 Agent 运行结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubAgentResult {
    /// 子 Agent 名称。
    pub name: String,
    /// 任务 ID。
    pub task_id: String,
    /// 最终状态。
    pub status: TaskStatus,
    /// 输出内容（成功时）。
    pub output: Option<String>,
    /// 错误信息（失败时）。
    pub error: Option<String>,
    /// 耗时秒数。
    pub duration_secs: f64,
}

/// 子 Agent 间通信消息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessage {
    /// 发送方（子 Agent 名称，或 "supervisor"）。
    pub sender: String,
    /// 主题（如 `sub_agent.result`、`supervisor.context`）。
    pub topic: String,
    /// 载荷。
    pub payload: serde_json::Value,
    /// 时间戳（RFC3339）。
    pub at: String,
}

/// 子 Agent 共享黑板：按会话隔离的消息总线。
///
/// 子 Agent 本身是 TaskRuntime 任务、无法回调，因此由运行时在子 Agent 进入终态时
/// **自动发布**结果消息；编排者（Supervisor）与后续子 Agent 通过 `read` / `read_topic`
/// 消费前序输出，实现"子 Agent 间通信"。
#[derive(Default)]
pub struct AgentMessageBus {
    board: Mutex<HashMap<String, Vec<AgentMessage>>>,
}

impl AgentMessageBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// 发布一条消息到指定会话。
    pub fn publish(&self, conversation: &str, message: AgentMessage) {
        if let Ok(mut board) = self.board.lock() {
            board.entry(conversation.to_owned()).or_default().push(message);
        }
    }

    /// 便捷发布：由 `sender` 向 `topic` 发送载荷。
    pub fn post(&self, conversation: &str, sender: &str, topic: &str, payload: serde_json::Value) {
        self.publish(
            conversation,
            AgentMessage {
                sender: sender.to_owned(),
                topic: topic.to_owned(),
                payload,
                at: time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
            },
        );
    }

    /// 读取指定会话的全部消息（按发布顺序）。
    pub fn read(&self, conversation: &str) -> Vec<AgentMessage> {
        self.board
            .lock()
            .ok()
            .and_then(|board| board.get(conversation).cloned())
            .unwrap_or_default()
    }

    /// 读取指定会话中某主题的消息。
    pub fn read_topic(&self, conversation: &str, topic: &str) -> Vec<AgentMessage> {
        self.read(conversation)
            .into_iter()
            .filter(|m| m.topic == topic)
            .collect()
    }

    /// 清空指定会话的黑板。
    pub fn clear(&self, conversation: &str) {
        if let Ok(mut board) = self.board.lock() {
            board.remove(conversation);
        }
    }
}

/// 活跃子 Agent 记录：任务句柄 + 启动时间（用于聚合耗时）。
struct ActiveAgent {
    handle: TaskHandle,
    started_at: Instant,
    /// 所属会话（用于结果消息落到对应黑板）。
    conversation_id: Option<String>,
}

/// 子 Agent 运行时：统一子 Agent 生命周期管理。
pub struct SubAgentRuntime {
    task_runtime: Arc<dyn TaskRuntime>,
    /// 活跃子 Agent：name → 记录。
    active_agents: Mutex<HashMap<String, ActiveAgent>>,
    event_bus: Option<Arc<EventBus>>,
    /// 等待结果时的轮询间隔（默认 500ms）。
    poll_interval: Duration,
    /// 默认等待超时（秒，默认 600）。
    default_timeout_secs: u64,
    /// 子 Agent 间通信黑板。
    bus: Arc<AgentMessageBus>,
}

impl SubAgentRuntime {
    pub fn new(task_runtime: Arc<dyn TaskRuntime>) -> Self {
        Self {
            task_runtime,
            active_agents: Mutex::new(HashMap::new()),
            event_bus: None,
            poll_interval: Duration::from_millis(500),
            default_timeout_secs: 600,
            bus: Arc::new(AgentMessageBus::new()),
        }
    }

    /// 子 Agent 间通信黑板（供编排者读取 / 注入上下文）。
    pub fn bus(&self) -> &Arc<AgentMessageBus> {
        &self.bus
    }

    /// 注入统一事件总线。
    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// 覆盖轮询间隔与默认超时（测试 / 调试用）。
    pub fn with_polling(mut self, poll_interval: Duration, default_timeout_secs: u64) -> Self {
        self.poll_interval = poll_interval;
        self.default_timeout_secs = default_timeout_secs;
        self
    }

    /// 提交子 Agent 任务。
    ///
    /// 返回 TaskHandle，可通过 `status()` 查询进度。
    pub fn spawn(&self, request: &SubAgentRequest) -> Result<TaskHandle, TaskError> {
        let task_request = TaskRequest {
            kind: request.agent_type.to_task_kind(),
            prompt: request.prompt.clone(),
            priority: request.priority,
            workspace_id: request.workspace_id.clone(),
            conversation_id: request.conversation_id.clone(),
            parameters: request.parameters.clone(),
            max_retries: request.max_retries,
            timeout_secs: request.timeout_secs,
            sandbox: None,
        };

        let handle = self.task_runtime.submit(&task_request)?;

        // 记录活跃子 Agent（含启动时间，供聚合耗时）。
        if let Ok(mut agents) = self.active_agents.lock() {
            agents.insert(
                request.name.clone(),
                ActiveAgent {
                    handle: handle.clone(),
                    started_at: Instant::now(),
                    conversation_id: request.conversation_id.clone(),
                },
            );
        }

        eprintln!(
            "[SubAgent] spawned name={} task_id={} kind={:?}",
            request.name, handle.task_id, request.agent_type
        );

        Ok(handle)
    }

    /// 查询子 Agent 状态。
    pub fn status(&self, name: &str) -> Option<TaskHandle> {
        self.active_agents
            .lock()
            .ok()
            .and_then(|agents| agents.get(name).map(|a| a.handle.clone()))
    }

    /// 取消子 Agent。
    pub fn cancel(&self, name: &str) -> Result<(), TaskError> {
        let handle = self
            .active_agents
            .lock()
            .map_err(|_| TaskError::NotFound(name.to_owned()))?
            .get(name)
            .map(|a| a.handle.clone())
            .ok_or(TaskError::NotFound(name.to_owned()))?;

        self.task_runtime.cancel(&handle.task_id)?;

        if let Ok(mut agents) = self.active_agents.lock() {
            agents.remove(name);
        }

        eprintln!(
            "[SubAgent] cancelled name={} task_id={}",
            name, handle.task_id
        );
        Ok(())
    }

    /// 列出所有活跃子 Agent。
    pub fn list_active(&self) -> Vec<(String, TaskHandle)> {
        self.active_agents
            .lock()
            .ok()
            .map(|agents| {
                agents
                    .iter()
                    .map(|(k, v)| (k.clone(), v.handle.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 清理已进入终态的子 Agent。
    pub fn cleanup_completed(&self) {
        if let Ok(mut agents) = self.active_agents.lock() {
            agents.retain(|_name, agent| agent.handle.status.is_active());
        }
    }

    // ─── 结果聚合（最小闭环：spawn → await → result）───

    /// 等待指定子 Agent 进入终态并返回聚合结果。
    ///
    /// - 轮询 `TaskRuntime::status`，直到 Completed / Failed / Cancelled / TimedOut 或超时。
    /// - 成功时从任务句柄的 `metadata["output"]` 提取输出（同步型运行时如 LLM 任务会在 submit 时写入）。
    /// - 终态后从活跃表移除；超时返回 `TaskError::Timeout`（不移除，便于重试 / 取消）。
    pub fn await_result(&self, name: &str, timeout_secs: u64) -> Result<SubAgentResult, TaskError> {
        let (handle, started_at, conversation_id) = self
            .active_agents
            .lock()
            .map_err(|_| TaskError::NotFound(name.to_owned()))?
            .get(name)
            .map(|agent| {
                (
                    agent.handle.clone(),
                    agent.started_at,
                    agent.conversation_id.clone(),
                )
            })
            .ok_or_else(|| TaskError::NotFound(name.to_owned()))?;

        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        loop {
            let status = self.task_runtime.status(&handle.task_id)?;
            if status.is_terminal() {
                let output = handle
                    .metadata
                    .get("output")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
                let error = if matches!(status, TaskStatus::Completed) {
                    None
                } else {
                    Some(
                        handle
                            .metadata
                            .get("error")
                            .and_then(|v| v.as_str())
                            .unwrap_or("子 Agent 未成功完成")
                            .to_owned(),
                    )
                };
                if let Ok(mut agents) = self.active_agents.lock() {
                    agents.remove(name);
                }
                let result = SubAgentResult {
                    name: name.to_owned(),
                    task_id: handle.task_id.clone(),
                    status,
                    output,
                    error,
                    duration_secs: started_at.elapsed().as_secs_f64(),
                };
                // 自动把结果发布到黑板，供后续子 Agent / Supervisor 消费。
                if let Some(conversation) = conversation_id.as_deref() {
                    self.bus.post(
                        conversation,
                        name,
                        "sub_agent.result",
                        serde_json::to_value(&result).unwrap_or(serde_json::Value::Null),
                    );
                }
                return Ok(result);
            }
            if Instant::now() >= deadline {
                return Err(TaskError::Timeout(timeout_secs));
            }
            std::thread::sleep(self.poll_interval);
        }
    }

    /// 提交并等待：spawn + await_result 的便捷组合（超时取请求值或默认值）。
    pub fn spawn_and_wait(&self, request: &SubAgentRequest) -> Result<SubAgentResult, TaskError> {
        let timeout = request.timeout_secs.unwrap_or(self.default_timeout_secs);
        self.spawn(request)?;
        self.await_result(&request.name, timeout)
    }

    /// 等待所有活跃子 Agent 完成并聚合结果（超时的条目被跳过）。
    pub fn collect_results(&self, timeout_secs: u64) -> Vec<SubAgentResult> {
        let names: Vec<String> = self
            .list_active()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        names
            .into_iter()
            .filter_map(|name| self.await_result(&name, timeout_secs).ok())
            .collect()
    }

    /// 批量并发提交（任务在 TaskRuntime 内并发执行），返回各句柄。
    pub fn spawn_batch(&self, requests: &[SubAgentRequest]) -> Result<Vec<TaskHandle>, TaskError> {
        requests.iter().map(|request| self.spawn(request)).collect()
    }

    /// 批量提交并等待全部完成（结果按请求顺序返回）。
    ///
    /// 任务在提交后即并发运行，等待虽为顺序进行，但总耗时≈最慢者而非累加。
    pub fn spawn_batch_and_wait(&self, requests: &[SubAgentRequest]) -> Vec<SubAgentResult> {
        for request in requests {
            if let Err(e) = self.spawn(request) {
                eprintln!("[SubAgent] batch spawn failed name={}: {e}", request.name);
            }
        }
        requests
            .iter()
            .filter_map(|request| {
                let timeout = request.timeout_secs.unwrap_or(self.default_timeout_secs);
                self.await_result(&request.name, timeout).ok()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// 桩 TaskRuntime：按预设状态序列返回，用于验证轮询与聚合。
    struct StubTaskRuntime {
        statuses: Mutex<VecDeque<TaskStatus>>,
        last: Mutex<TaskStatus>,
        output: Option<String>,
    }

    impl StubTaskRuntime {
        fn new(statuses: Vec<TaskStatus>, output: Option<String>) -> Self {
            Self {
                statuses: Mutex::new(statuses.into()),
                last: Mutex::new(TaskStatus::Pending),
                output,
            }
        }
    }

    impl TaskRuntime for StubTaskRuntime {
        fn submit(&self, _request: &TaskRequest) -> Result<TaskHandle, TaskError> {
            let mut metadata = HashMap::new();
            if let Some(output) = &self.output {
                metadata.insert(
                    "output".to_owned(),
                    serde_json::Value::String(output.clone()),
                );
            }
            Ok(TaskHandle {
                task_id: "stub-task".to_owned(),
                status: TaskStatus::Pending,
                provider_id: None,
                model_name: None,
                remote_job_id: None,
                metadata,
            })
        }

        fn status(&self, _task_id: &str) -> Result<TaskStatus, TaskError> {
            let next = self
                .statuses
                .lock()
                .ok()
                .and_then(|mut s| s.pop_front())
                .unwrap_or(*self.last.lock().unwrap());
            if let Ok(mut last) = self.last.lock() {
                *last = next;
            }
            Ok(next)
        }

        fn cancel(&self, _task_id: &str) -> Result<bool, TaskError> {
            Ok(true)
        }
    }

    fn sample_request(name: &str) -> SubAgentRequest {
        SubAgentRequest {
            name: name.to_owned(),
            agent_type: SubAgentType::Analyzer,
            prompt: "分析这段需求".to_owned(),
            priority: TaskPriority::default(),
            workspace_id: "ws-1".to_owned(),
            conversation_id: None,
            parameters: HashMap::new(),
            max_retries: 0,
            timeout_secs: Some(2),
        }
    }

    #[test]
    fn await_result_returns_completed_with_output() {
        let runtime = Arc::new(StubTaskRuntime::new(
            vec![TaskStatus::Running, TaskStatus::Completed],
            Some("分析结果".to_owned()),
        ));
        let sub = SubAgentRuntime::new(runtime).with_polling(Duration::from_millis(1), 2);
        sub.spawn(&sample_request("analyzer")).unwrap();

        let result = sub.await_result("analyzer", 2).unwrap();
        assert_eq!(result.status, TaskStatus::Completed);
        assert_eq!(result.output.as_deref(), Some("分析结果"));
        assert!(result.error.is_none());
        // 终态后应从活跃表移除
        assert!(sub.status("analyzer").is_none());
    }

    #[test]
    fn await_result_times_out_when_never_terminal() {
        let runtime = Arc::new(StubTaskRuntime::new(vec![], None));
        let sub = SubAgentRuntime::new(runtime).with_polling(Duration::from_millis(1), 1);
        sub.spawn(&sample_request("slow")).unwrap();

        let err = sub.await_result("slow", 1).unwrap_err();
        assert!(matches!(err, TaskError::Timeout(1)));
        // 超时不移除，仍可查询 / 取消
        assert!(sub.status("slow").is_some());
    }

    #[test]
    fn spawn_and_wait_completes() {
        let runtime = Arc::new(StubTaskRuntime::new(vec![TaskStatus::Completed], None));
        let sub = SubAgentRuntime::new(runtime).with_polling(Duration::from_millis(1), 5);
        let result = sub.spawn_and_wait(&sample_request("fast")).unwrap();
        assert_eq!(result.status, TaskStatus::Completed);
    }

    #[test]
    fn batch_spawn_and_wait_returns_all_results() {
        let runtime = Arc::new(StubTaskRuntime::new(
            vec![TaskStatus::Completed],
            Some("out".to_owned()),
        ));
        let sub = SubAgentRuntime::new(runtime).with_polling(Duration::from_millis(1), 5);
        let requests = vec![sample_request("a"), sample_request("b")];
        let results = sub.spawn_batch_and_wait(&requests);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.status == TaskStatus::Completed));
    }

    #[test]
    fn result_is_published_to_bus() {
        let runtime = Arc::new(StubTaskRuntime::new(
            vec![TaskStatus::Completed],
            Some("分析".to_owned()),
        ));
        let sub = SubAgentRuntime::new(runtime).with_polling(Duration::from_millis(1), 5);
        let mut request = sample_request("analyzer");
        request.conversation_id = Some("conv-1".to_owned());
        sub.spawn(&request).unwrap();
        let _ = sub.await_result("analyzer", 5).unwrap();

        let messages = sub.bus().read_topic("conv-1", "sub_agent.result");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sender, "analyzer");
        assert_eq!(
            messages[0].payload.get("output").and_then(|v| v.as_str()),
            Some("分析")
        );
    }

    #[test]
    fn bus_post_and_read_roundtrip() {
        let runtime = Arc::new(StubTaskRuntime::new(vec![], None));
        let sub = SubAgentRuntime::new(runtime);
        sub.bus().post(
            "conv-2",
            "supervisor",
            "supervisor.context",
            serde_json::json!({ "goal": "春节宣传片" }),
        );
        let messages = sub.bus().read("conv-2");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].topic, "supervisor.context");
        assert_eq!(messages[0].sender, "supervisor");
    }
}
