use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 创作工作流阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStage {
    /// 生成中
    Generating,
    /// 自动评价中
    Evaluating,
    /// 等待用户反馈
    WaitingFeedback,
    /// 解析反馈中
    ParsingFeedback,
    /// 生成修改计划
    PlanningModification,
    /// 执行修改
    ApplyingModification,
    /// 重新生成中
    Regenerating,
    /// 完成
    Completed,
    /// 失败
    Failed,
}

impl WorkflowStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Generating => "generating",
            Self::Evaluating => "evaluating",
            Self::WaitingFeedback => "waiting_feedback",
            Self::ParsingFeedback => "parsing_feedback",
            Self::PlanningModification => "planning_modification",
            Self::ApplyingModification => "applying_modification",
            Self::Regenerating => "regenerating",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Result<Self, WorkflowError> {
        match value {
            "generating" => Ok(Self::Generating),
            "evaluating" => Ok(Self::Evaluating),
            "waiting_feedback" => Ok(Self::WaitingFeedback),
            "parsing_feedback" => Ok(Self::ParsingFeedback),
            "planning_modification" => Ok(Self::PlanningModification),
            "applying_modification" => Ok(Self::ApplyingModification),
            "regenerating" => Ok(Self::Regenerating),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            _ => Err(WorkflowError::InvalidStage(value.to_owned())),
        }
    }

    /// 阶段是否为终态。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }

    /// 阶段是否为等待用户输入。
    pub fn is_waiting(self) -> bool {
        matches!(self, Self::WaitingFeedback)
    }
}

/// 工作流事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowEventType {
    StageEntered,
    StageCompleted,
    StageFailed,
    AutoEvaluationCompleted,
    FeedbackReceived,
    ModificationPlanCreated,
    ModificationApplied,
    RegenerationTriggered,
    WorkflowCompleted,
    WorkflowFailed,
}

impl WorkflowEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StageEntered => "stage_entered",
            Self::StageCompleted => "stage_completed",
            Self::StageFailed => "stage_failed",
            Self::AutoEvaluationCompleted => "auto_evaluation_completed",
            Self::FeedbackReceived => "feedback_received",
            Self::ModificationPlanCreated => "modification_plan_created",
            Self::ModificationApplied => "modification_applied",
            Self::RegenerationTriggered => "regeneration_triggered",
            Self::WorkflowCompleted => "workflow_completed",
            Self::WorkflowFailed => "workflow_failed",
        }
    }
}

/// 工作流事件记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowEvent {
    pub event_type: WorkflowEventType,
    pub stage: WorkflowStage,
    pub detail: Option<String>,
    pub timestamp: String,
    pub review_report_id: Option<String>,
    pub edit_request_id: Option<String>,
    pub edit_plan_id: Option<String>,
}

/// 工作流状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowState {
    /// 工作流 ID
    pub id: String,
    /// 关联的项目 ID
    pub project_id: String,
    /// 关联的资产 ID
    pub asset_id: String,
    /// 关联的镜头 ID（可选）
    pub shot_id: Option<String>,
    /// 当前阶段
    pub current_stage: WorkflowStage,
    /// 当前迭代次数
    pub iteration: u32,
    /// 最大迭代次数
    pub max_iterations: u32,
    /// 当前评价报告 ID
    pub current_review_id: Option<String>,
    /// 当前编辑计划 ID
    pub current_plan_id: Option<String>,
    /// 事件历史
    pub events: Vec<WorkflowEvent>,
    /// 是否已暂停（等待用户反馈）
    pub is_paused: bool,
    /// 错误信息
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 工作流配置。
#[derive(Debug, Clone)]
pub struct WorkflowConfig {
    /// 最大迭代次数（默认 3）
    pub max_iterations: u32,
    /// 是否启用自动评价（默认 true）
    pub auto_evaluate: bool,
    /// 是否启用自动重新生成（默认 false，需用户确认）
    pub auto_regenerate: bool,
    /// 视频生成门槛分数（默认 80.0）
    pub video_gate_threshold: f64,
    /// 是否暂停等待用户反馈（默认 true）
    pub pause_for_feedback: bool,
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            max_iterations: 3,
            auto_evaluate: true,
            auto_regenerate: false,
            video_gate_threshold: 80.0,
            pause_for_feedback: true,
        }
    }
}

/// 工作流错误。
#[derive(Debug, Clone, Error)]
pub enum WorkflowError {
    #[error("invalid workflow stage: {0}")]
    InvalidStage(String),
    #[error("workflow is already in terminal stage: {stage:?}")]
    AlreadyTerminal { stage: WorkflowStage },
    #[error("workflow is not waiting for feedback")]
    NotWaitingFeedback,
    #[error("workflow iteration limit reached: {0}")]
    IterationLimitReached(u32),
    #[error("evaluation failed: {0}")]
    EvaluationFailed(String),
    #[error("modification failed: {0}")]
    ModificationFailed(String),
}
