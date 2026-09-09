#![allow(dead_code)]
//! Task Runtime 端口：统一任务执行接口。
//!
//! 定义 TaskRuntime trait，所有长耗时任务（生成、合成、分析等）
//! 通过此接口提交、查询、取消。
//!
//! 职责边界：
//! - TaskRuntime 负责：任务生命周期管理（提交→轮询→完成/失败）
//! - 实现方负责：将 TaskRequest 映射到具体 Provider/Worker
//! - 调用方负责：构建 TaskRequest、处理 TaskResult

use std::collections::HashMap;

use crate::domain::task::{TaskKind, TaskPriority, TaskStatus};

// ─── TaskRequest ───

/// 任务提交请求。
#[derive(Debug, Clone)]
pub struct TaskRequest {
    /// 任务类型。
    pub kind: TaskKind,
    /// 任务提示词 / 描述。
    pub prompt: String,
    /// 优先级。
    pub priority: TaskPriority,
    /// 关联的工作区 ID。
    pub workspace_id: String,
    /// 关联的会话 ID（可选）。
    pub conversation_id: Option<String>,
    /// 附加参数（ratio, resolution, duration, style, reference_url 等）。
    pub parameters: HashMap<String, serde_json::Value>,
    /// 最大重试次数（默认 2）。
    pub max_retries: u32,
    /// 超时秒数（默认由 TaskKind 决定）。
    pub timeout_secs: Option<u64>,
    /// 沙箱约束（可选，Phase 3 接入）。
    pub sandbox: Option<crate::domain::sandbox::SandboxPolicy>,
}

impl TaskRequest {
    /// 创建简单任务请求。
    pub fn simple(
        kind: TaskKind,
        prompt: impl Into<String>,
        workspace_id: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            prompt: prompt.into(),
            priority: TaskPriority::default(),
            workspace_id: workspace_id.into(),
            conversation_id: None,
            parameters: HashMap::new(),
            max_retries: 2,
            timeout_secs: None,
            sandbox: None,
        }
    }

    /// 设置参数。
    pub fn with_param(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.parameters.insert(key.into(), value);
        self
    }

    /// 设置会话 ID。
    pub fn with_conversation(mut self, conversation_id: impl Into<String>) -> Self {
        self.conversation_id = Some(conversation_id.into());
        self
    }

    /// 设置优先级。
    pub fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }
}

// ─── TaskHandle ───

/// 任务提交返回值。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskHandle {
    /// 任务 ID。
    pub task_id: String,
    /// 提交后的初始状态。
    pub status: TaskStatus,
    /// Provider 使用的 provider_id（如果已确定）。
    pub provider_id: Option<String>,
    /// Provider 使用的 model_name（如果已确定）。
    pub model_name: Option<String>,
    /// 远端 Job ID（异步任务）。
    pub remote_job_id: Option<String>,
    /// 附加元数据。
    pub metadata: HashMap<String, serde_json::Value>,
}

// ─── TaskError ───

/// Task Runtime 错误。
#[derive(Debug, Clone, thiserror::Error)]
pub enum TaskError {
    #[error("unknown task kind: {0}")]
    UnknownKind(String),

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("no available provider for task kind: {0:?}")]
    NoProvider(TaskKind),

    #[error("submission failed: {0}")]
    SubmissionFailed(String),

    #[error("task not found: {0}")]
    NotFound(String),

    #[error("task is not in a cancellable state: {task_id} (status: {status:?})")]
    NotCancellable { task_id: String, status: TaskStatus },

    #[error("timeout after {0} seconds")]
    Timeout(u64),

    #[error("persistence error: {0}")]
    Persistence(String),

    #[error("provider error: {0}")]
    Provider(String),
}

// ─── TaskRuntime Trait ───

/// 统一任务运行时接口。
///
/// 所有长耗时任务通过此接口提交。
/// 实现方将 TaskRequest 映射到具体基础设施（GenerationSubmitService + PollWorker 等）。
pub trait TaskRuntime: Send + Sync {
    /// 提交任务。返回 TaskHandle 包含任务 ID 和初始状态。
    fn submit(&self, request: &TaskRequest) -> Result<TaskHandle, TaskError>;

    /// 查询任务当前状态。
    fn status(&self, task_id: &str) -> Result<TaskStatus, TaskError>;

    /// 取消任务。返回是否成功取消。
    fn cancel(&self, task_id: &str) -> Result<bool, TaskError>;

    /// 获取活跃任务数量。
    fn active_count(&self) -> usize {
        0
    }
}
