#![allow(dead_code)]
//! Task 领域模型：统一执行单元。
//!
//! 将 GenerationAttempt、ExecutionStep、WorkflowStage 等执行概念
//! 统一为 Task 抽象，为 TaskRuntime 提供领域基础。
//!
//! 设计原则：
//! - Task 是原子执行单元（一次生成、一次合成、一次导出）
//! - Task 有明确的生命周期（Pending → Running → Completed/Failed/Cancelled）
//! - Task 可被调度、重试、取消、超时
//! - Task 产生 Artifact

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ─── TaskStatus ───

/// 任务状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// 已创建，等待执行。
    Pending,
    /// 执行中。
    Running,
    /// 已完成。
    Completed,
    /// 失败。
    Failed,
    /// 已取消。
    Cancelled,
    /// 超时。
    TimedOut,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "timed_out" => Some(Self::TimedOut),
            _ => None,
        }
    }

    /// 是否为终态（不可再转换）。
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::TimedOut
        )
    }

    /// 是否为活跃状态（仍在执行或等待执行）。
    pub fn is_active(self) -> bool {
        matches!(self, Self::Pending | Self::Running)
    }
}

// ─── TaskKind ───

/// 任务类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// 图片生成。
    ImageGeneration,
    /// 视频生成。
    VideoGeneration,
    /// 音频生成（TTS / Music）。
    AudioGeneration,
    /// 多素材合成（ffmpeg）。
    Composition,
    /// 风格分析 / Vision 分析。
    Analysis,
    /// 资产导入。
    Import,
    /// 通用任务。
    Generic,
}

impl TaskKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ImageGeneration => "image_generation",
            Self::VideoGeneration => "video_generation",
            Self::AudioGeneration => "audio_generation",
            Self::Composition => "composition",
            Self::Analysis => "analysis",
            Self::Import => "import",
            Self::Generic => "generic",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "image_generation" => Some(Self::ImageGeneration),
            "video_generation" => Some(Self::VideoGeneration),
            "audio_generation" => Some(Self::AudioGeneration),
            "composition" => Some(Self::Composition),
            "analysis" => Some(Self::Analysis),
            "import" => Some(Self::Import),
            "generic" => Some(Self::Generic),
            _ => None,
        }
    }

    /// 默认优先级（数值越小越优先）。
    pub fn default_priority(self) -> u32 {
        match self {
            Self::Analysis => 0,
            Self::ImageGeneration => 1,
            Self::AudioGeneration => 1,
            Self::VideoGeneration => 2,
            Self::Composition => 3,
            Self::Import => 4,
            Self::Generic => 5,
        }
    }

    /// 默认超时时间（秒）。
    pub fn default_timeout_secs(self) -> u64 {
        match self {
            Self::ImageGeneration => 120,
            Self::VideoGeneration => 600,
            Self::AudioGeneration => 120,
            Self::Composition => 300,
            Self::Analysis => 60,
            Self::Import => 60,
            Self::Generic => 300,
        }
    }
}

// ─── TaskPriority ───

/// 任务优先级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TaskPriority {
    /// 紧急（用户交互阻塞）。
    Critical = 0,
    /// 高优先级。
    High = 1,
    /// 正常（默认）。
    #[default]
    Normal = 2,
    /// 低优先级（后台任务）。
    Low = 3,
}

// ─── TaskRecord ───

/// 任务记录：一个原子执行单元。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    /// 任务 ID。
    pub id: String,
    /// 任务类型。
    pub kind: TaskKind,
    /// 当前状态。
    pub status: TaskStatus,
    /// 优先级。
    pub priority: TaskPriority,
    /// 关联的工作区 ID。
    pub workspace_id: String,
    /// 关联的会话 ID（可选）。
    pub conversation_id: Option<String>,
    /// 关联的计划步骤 ID（可选）。
    pub plan_step_id: Option<String>,
    /// 任务提示词 / 描述。
    pub prompt: String,
    /// 附加参数。
    pub parameters: HashMap<String, serde_json::Value>,
    /// 使用的 Provider ID。
    pub provider_id: Option<String>,
    /// 使用的模型名。
    pub model_name: Option<String>,
    /// 远端 Job ID（异步任务用）。
    pub remote_job_id: Option<String>,
    /// 产生的 Artifact ID 列表。
    pub artifact_ids: Vec<String>,
    /// 错误信息（失败时）。
    pub error: Option<String>,
    /// 进度百分比（0-100）。
    pub progress: u8,
    /// 重试次数。
    pub retry_count: u32,
    /// 最大重试次数。
    pub max_retries: u32,
    /// 创建时间。
    pub created_at: String,
    /// 开始执行时间。
    pub started_at: Option<String>,
    /// 完成时间。
    pub completed_at: Option<String>,
    /// 最后更新时间。
    pub updated_at: String,
}

// ─── TaskDraft ───

/// 创建任务的草稿。
#[derive(Debug, Clone)]
pub struct TaskDraft {
    pub kind: TaskKind,
    pub priority: TaskPriority,
    pub workspace_id: String,
    pub conversation_id: Option<String>,
    pub plan_step_id: Option<String>,
    pub prompt: String,
    pub parameters: HashMap<String, serde_json::Value>,
    pub max_retries: u32,
}

// ─── TaskResult ───

/// 任务执行结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResult {
    /// 任务 ID。
    pub task_id: String,
    /// 最终状态。
    pub status: TaskStatus,
    /// 产生的 Artifact ID 列表。
    pub artifact_ids: Vec<String>,
    /// 错误信息（失败时）。
    pub error: Option<String>,
    /// 执行耗时（秒）。
    pub duration_secs: f64,
    /// 附加元数据。
    pub metadata: HashMap<String, serde_json::Value>,
}

// ─── TaskEvent ───

/// 任务生命周期事件（用于 EventBus）。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskEvent {
    /// 任务已创建。
    Created { task_id: String, kind: TaskKind },
    /// 任务开始执行。
    Started { task_id: String },
    /// 任务进度更新。
    ProgressUpdated {
        task_id: String,
        progress: u8,
        message: Option<String>,
    },
    /// 任务状态变更。
    StatusChanged {
        task_id: String,
        old_status: TaskStatus,
        new_status: TaskStatus,
    },
    /// 任务已完成。
    Completed {
        task_id: String,
        artifact_ids: Vec<String>,
        duration_secs: f64,
    },
    /// 任务失败。
    Failed { task_id: String, error: String },
    /// 任务已取消。
    Cancelled { task_id: String },
}

impl TaskEvent {
    pub fn task_id(&self) -> &str {
        match self {
            Self::Created { task_id, .. }
            | Self::Started { task_id }
            | Self::ProgressUpdated { task_id, .. }
            | Self::StatusChanged { task_id, .. }
            | Self::Completed { task_id, .. }
            | Self::Failed { task_id, .. }
            | Self::Cancelled { task_id } => task_id,
        }
    }
}

// ─── Helpers ───

/// 生成任务 ID。
pub fn generate_task_id() -> String {
    format!("task-{}", uuid::Uuid::new_v4())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_status_lifecycle() {
        assert!(TaskStatus::Pending.is_active());
        assert!(TaskStatus::Running.is_active());
        assert!(!TaskStatus::Completed.is_active());
        assert!(!TaskStatus::Failed.is_active());

        assert!(!TaskStatus::Pending.is_terminal());
        assert!(!TaskStatus::Running.is_terminal());
        assert!(TaskStatus::Completed.is_terminal());
        assert!(TaskStatus::Failed.is_terminal());
        assert!(TaskStatus::Cancelled.is_terminal());
        assert!(TaskStatus::TimedOut.is_terminal());
    }

    #[test]
    fn test_task_status_serde() {
        let status = TaskStatus::Running;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, r#""running""#);

        let parsed: TaskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, TaskStatus::Running);
    }

    #[test]
    fn test_task_kind_default_timeout() {
        assert_eq!(TaskKind::ImageGeneration.default_timeout_secs(), 120);
        assert_eq!(TaskKind::VideoGeneration.default_timeout_secs(), 600);
        assert_eq!(TaskKind::Composition.default_timeout_secs(), 300);
    }

    #[test]
    fn test_task_kind_default_priority() {
        assert!(
            TaskKind::Analysis.default_priority() < TaskKind::ImageGeneration.default_priority()
        );
        assert!(
            TaskKind::ImageGeneration.default_priority()
                < TaskKind::VideoGeneration.default_priority()
        );
        assert!(
            TaskKind::VideoGeneration.default_priority() < TaskKind::Composition.default_priority()
        );
    }

    #[test]
    fn test_task_event_serde() {
        let event = TaskEvent::Completed {
            task_id: "task-001".to_owned(),
            artifact_ids: vec!["art-001".to_owned()],
            duration_secs: 45.3,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"completed""#));
        assert!(json.contains(r#""task_id":"task-001""#));
    }

    #[test]
    fn test_generate_task_id() {
        let id = generate_task_id();
        assert!(id.starts_with("task-"));
        assert!(id.len() > 5);
    }
}
