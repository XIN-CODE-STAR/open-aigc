//! LLM Task Runtime：TaskRuntime 的文本任务实现。
//!
//! 为 `TaskKind::Analysis` / `TaskKind::Generic` 提供 LLM 执行路径，
//! 使 Analyzer / Generic 子 Agent 能真正运行（此前无任何 TaskRuntime 处理这两类，
//! `SubAgentRuntime` 提交后必然 `UnknownKind`）。
//!
//! 设计：`submit()` 同步调用 LLM 并立即完成，输出写入 `TaskHandle.metadata["output"]`，
//! 供 `SubAgentRuntime::await_result` 聚合。同步阻塞是可接受的——调用方（子 Agent 编排）
//! 本就运行在后台线程。
//!
//! 另含 `CompositeTaskRuntime`：按 `TaskKind` 将生成类任务路由到生成运行时、
//! 文本类任务路由到 LLM 运行时。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::domain::agent::{ChatMessage, ChatRequest};
use crate::domain::task::{TaskKind, TaskStatus};
use crate::ports::agent_llm::AgentLlm;
use crate::ports::task_runtime::{TaskError, TaskHandle, TaskRequest, TaskRuntime};

/// 子 Agent 的系统提示词：只完成被指派任务，直接给结论。
const SUB_AGENT_SYSTEM: &str =
    "你是一个专注于当前子任务的执行助手。只完成被指派的任务，直接给出结论，不要寒暄。";

/// LLM 文本任务运行时。
pub struct LlmTaskRuntime {
    llm: Arc<Mutex<Box<dyn AgentLlm>>>,
    model: String,
    max_iterations: u8,
}

impl LlmTaskRuntime {
    /// 用已就绪的 LLM 适配器（含凭据）构造。
    pub fn new(llm: Box<dyn AgentLlm>, model: impl Into<String>) -> Self {
        Self {
            llm: Arc::new(Mutex::new(llm)),
            model: model.into(),
            max_iterations: 1,
        }
    }

    /// 设置 LLM 循环上限（子 Agent 默认单轮）。
    pub fn with_max_iterations(mut self, max_iterations: u8) -> Self {
        self.max_iterations = max_iterations;
        self
    }
}

impl TaskRuntime for LlmTaskRuntime {
    fn submit(&self, request: &TaskRequest) -> Result<TaskHandle, TaskError> {
        match request.kind {
            TaskKind::Analysis | TaskKind::Generic => {}
            other => {
                return Err(TaskError::UnknownKind(format!(
                    "{other:?} is not a text task"
                )));
            }
        }

        let messages = vec![
            ChatMessage::system(SUB_AGENT_SYSTEM.to_owned()),
            ChatMessage::user(request.prompt.clone()),
        ];
        let mut chat_request = ChatRequest::new(self.model.clone(), messages, Vec::new());
        chat_request.max_iterations = self.max_iterations;

        let mut llm = self
            .llm
            .lock()
            .map_err(|_| TaskError::SubmissionFailed("llm lock poisoned".to_owned()))?;
        let response = llm
            .chat(&chat_request)
            .map_err(|e| TaskError::Provider(e.to_string()))?;

        let output = response.content.unwrap_or_default();
        let mut metadata = HashMap::new();
        metadata.insert("output".to_owned(), serde_json::Value::String(output));

        Ok(TaskHandle {
            task_id: format!("llm-{}", uuid::Uuid::new_v4()),
            status: TaskStatus::Completed,
            provider_id: None,
            model_name: Some(self.model.clone()),
            remote_job_id: None,
            metadata,
        })
    }

    fn status(&self, _task_id: &str) -> Result<TaskStatus, TaskError> {
        // 同步运行时：submit 返回即已完成。
        Ok(TaskStatus::Completed)
    }

    fn cancel(&self, task_id: &str) -> Result<bool, TaskError> {
        Err(TaskError::NotCancellable {
            task_id: task_id.to_owned(),
            status: TaskStatus::Completed,
        })
    }
}

/// 组合任务运行时：按 `TaskKind` 分派。
///
/// - 生成类（Image/Video/Audio）→ `generation`
/// - 其余（Analysis/Generic）→ `text`
pub struct CompositeTaskRuntime {
    generation: Arc<dyn TaskRuntime>,
    text: Arc<dyn TaskRuntime>,
}

impl CompositeTaskRuntime {
    pub fn new(generation: Arc<dyn TaskRuntime>, text: Arc<dyn TaskRuntime>) -> Self {
        Self { generation, text }
    }

    /// 判断是否属于生成类任务。
    fn is_generation(kind: TaskKind) -> bool {
        matches!(
            kind,
            TaskKind::ImageGeneration | TaskKind::VideoGeneration | TaskKind::AudioGeneration
        )
    }
}

impl TaskRuntime for CompositeTaskRuntime {
    fn submit(&self, request: &TaskRequest) -> Result<TaskHandle, TaskError> {
        if Self::is_generation(request.kind) {
            self.generation.submit(request)
        } else {
            self.text.submit(request)
        }
    }

    fn status(&self, task_id: &str) -> Result<TaskStatus, TaskError> {
        match self.generation.status(task_id) {
            Ok(status) => Ok(status),
            // 生成运行时找不到 → 回退到文本运行时
            Err(TaskError::NotFound(_)) => self.text.status(task_id),
            Err(e) => Err(e),
        }
    }

    fn cancel(&self, task_id: &str) -> Result<bool, TaskError> {
        match self.generation.cancel(task_id) {
            Ok(result) => Ok(result),
            Err(TaskError::NotFound(_)) | Err(TaskError::NotCancellable { .. }) => {
                self.text.cancel(task_id)
            }
            Err(e) => Err(e),
        }
    }

    fn active_count(&self) -> usize {
        self.generation.active_count() + self.text.active_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::agent::ChatResponse;
    use crate::ports::agent_llm::AgentLlmError;

    struct StubLlm {
        reply: String,
    }

    impl AgentLlm for StubLlm {
        fn chat(&mut self, _request: &ChatRequest) -> Result<ChatResponse, AgentLlmError> {
            Ok(ChatResponse {
                content: Some(self.reply.clone()),
                tool_calls: Vec::new(),
                finish_reason: "stop".to_owned(),
                model: "stub".to_owned(),
                prompt_tokens: None,
                completion_tokens: None,
            })
        }
    }

    fn request(kind: TaskKind) -> TaskRequest {
        TaskRequest::simple(kind, "写一段需求分析", "ws-1")
    }

    #[test]
    fn llm_runtime_rejects_generation_kind() {
        let runtime = LlmTaskRuntime::new(
            Box::new(StubLlm {
                reply: "x".to_owned(),
            }),
            "stub-model",
        );
        let err = runtime.submit(&request(TaskKind::ImageGeneration)).unwrap_err();
        assert!(matches!(err, TaskError::UnknownKind(_)));
    }

    #[test]
    fn llm_runtime_returns_completed_with_output() {
        let runtime = LlmTaskRuntime::new(
            Box::new(StubLlm {
                reply: "需求要点：时长 30s".to_owned(),
            }),
            "stub-model",
        );
        let handle = runtime.submit(&request(TaskKind::Analysis)).unwrap();
        assert_eq!(handle.status, TaskStatus::Completed);
        assert_eq!(
            handle.metadata.get("output").and_then(|v| v.as_str()),
            Some("需求要点：时长 30s")
        );
    }

    /// 记录收到的 kind 的桩运行时。
    struct RecordingRuntime {
        seen: Mutex<Vec<TaskKind>>,
    }

    impl TaskRuntime for RecordingRuntime {
        fn submit(&self, request: &TaskRequest) -> Result<TaskHandle, TaskError> {
            if let Ok(mut seen) = self.seen.lock() {
                seen.push(request.kind);
            }
            Ok(TaskHandle {
                task_id: "rec".to_owned(),
                status: TaskStatus::Running,
                provider_id: None,
                model_name: None,
                remote_job_id: None,
                metadata: HashMap::new(),
            })
        }
        fn status(&self, _task_id: &str) -> Result<TaskStatus, TaskError> {
            Ok(TaskStatus::Running)
        }
        fn cancel(&self, _task_id: &str) -> Result<bool, TaskError> {
            Ok(true)
        }
    }

    #[test]
    fn composite_routes_by_kind() {
        let generation = Arc::new(RecordingRuntime {
            seen: Mutex::new(Vec::new()),
        });
        let text = Arc::new(RecordingRuntime {
            seen: Mutex::new(Vec::new()),
        });
        let composite = CompositeTaskRuntime::new(generation.clone(), text.clone());

        composite.submit(&request(TaskKind::VideoGeneration)).unwrap();
        composite.submit(&request(TaskKind::Generic)).unwrap();

        assert_eq!(generation.seen.lock().unwrap().as_slice(), [TaskKind::VideoGeneration]);
        assert_eq!(text.seen.lock().unwrap().as_slice(), [TaskKind::Generic]);
    }
}
