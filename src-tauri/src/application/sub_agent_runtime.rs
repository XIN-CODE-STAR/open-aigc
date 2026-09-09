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

/// 子 Agent 运行时：统一子 Agent 生命周期管理。
pub struct SubAgentRuntime {
    task_runtime: Arc<dyn TaskRuntime>,
    /// 活跃子 Agent：name → TaskHandle。
    active_agents: Mutex<HashMap<String, TaskHandle>>,
    event_bus: Option<Arc<EventBus>>,
}

impl SubAgentRuntime {
    pub fn new(task_runtime: Arc<dyn TaskRuntime>) -> Self {
        Self {
            task_runtime,
            active_agents: Mutex::new(HashMap::new()),
            event_bus: None,
        }
    }

    /// 注入统一事件总线。
    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
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

        // 记录活跃子 Agent。
        if let Ok(mut agents) = self.active_agents.lock() {
            agents.insert(request.name.clone(), handle.clone());
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
            .and_then(|agents| agents.get(name).cloned())
    }

    /// 取消子 Agent。
    pub fn cancel(&self, name: &str) -> Result<(), TaskError> {
        let handle = self
            .active_agents
            .lock()
            .map_err(|_| TaskError::NotFound(name.to_owned()))?
            .get(name)
            .cloned()
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
            .map(|agents| agents.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default()
    }

    /// 清理已完成的子 Agent。
    pub fn cleanup_completed(&self) {
        if let Ok(mut agents) = self.active_agents.lock() {
            agents.retain(|_name, handle| {
                matches!(handle.status, TaskStatus::Pending | TaskStatus::Running)
            });
        }
    }
}
