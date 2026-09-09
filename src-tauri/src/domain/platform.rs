#![allow(dead_code)]
//! AI 视频/图片生成 Agent 平台领域模型。
//!
//! 按照 2026-07-20-ai-video-image-agent-platform-architecture.md 实现。

use serde::{Deserialize, Serialize};
use thiserror::Error;

const TITLE_MAX: usize = 200;
const TEXT_MAX: usize = 8000;
const NAME_MAX: usize = 120;

// ═══════════════════════════════════════════════════
// Project
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectType {
    PromoVideo,
    ShortFilm,
    ProductVideo,
    Documentary,
    SocialMedia,
    Educational,
}

impl ProjectType {
    pub fn parse(v: &str) -> Result<Self, PlatformError> {
        match v {
            "promo_video" => Ok(Self::PromoVideo),
            "short_film" => Ok(Self::ShortFilm),
            "product_video" => Ok(Self::ProductVideo),
            "documentary" => Ok(Self::Documentary),
            "social_media" => Ok(Self::SocialMedia),
            "educational" => Ok(Self::Educational),
            _ => Err(PlatformError::InvalidChoice("projectType")),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PromoVideo => "promo_video",
            Self::ShortFilm => "short_film",
            Self::ProductVideo => "product_video",
            Self::Documentary => "documentary",
            Self::SocialMedia => "social_media",
            Self::Educational => "educational",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectStatus {
    Draft,
    Planning,
    InProgress,
    Review,
    Completed,
    Archived,
}

impl ProjectStatus {
    pub fn parse(v: &str) -> Result<Self, PlatformError> {
        match v {
            "draft" => Ok(Self::Draft),
            "planning" => Ok(Self::Planning),
            "in_progress" => Ok(Self::InProgress),
            "review" => Ok(Self::Review),
            "completed" => Ok(Self::Completed),
            "archived" => Ok(Self::Archived),
            _ => Err(PlatformError::InvalidChoice("status")),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Planning => "planning",
            Self::InProgress => "in_progress",
            Self::Review => "review",
            Self::Completed => "completed",
            Self::Archived => "archived",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDraft {
    pub workspace_id: String,
    pub title: String,
    pub project_type: ProjectType,
    pub description: Option<String>,
    pub target_duration_seconds: Option<i32>,
    pub target_platform: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecord {
    pub id: String,
    pub workspace_id: String,
    pub owner_teacher_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub project_type: ProjectType,
    pub status: ProjectStatus,
    pub target_duration_seconds: Option<i32>,
    pub target_platform: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ═══════════════════════════════════════════════════
// CreativeRun
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunStatus {
    Pending,
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

impl RunStatus {
    pub fn parse(v: &str) -> Result<Self, PlatformError> {
        match v {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "waiting_approval" => Ok(Self::WaitingApproval),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(PlatformError::InvalidChoice("runStatus")),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::WaitingApproval => "waiting_approval",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStage {
    RequirementAnalysis,
    VisualSpec,
    StoryPlanning,
    CharacterPlanning,
    HumanApproval,
    ImageGeneration,
    VideoGeneration,
    Review,
    Optimization,
    Editing,
    FinalReview,
    Export,
}

impl WorkflowStage {
    pub fn parse(v: &str) -> Result<Self, PlatformError> {
        match v {
            "requirement_analysis" => Ok(Self::RequirementAnalysis),
            "visual_spec" => Ok(Self::VisualSpec),
            "story_planning" => Ok(Self::StoryPlanning),
            "character_planning" => Ok(Self::CharacterPlanning),
            "human_approval" => Ok(Self::HumanApproval),
            "image_generation" => Ok(Self::ImageGeneration),
            "video_generation" => Ok(Self::VideoGeneration),
            "review" => Ok(Self::Review),
            "optimization" => Ok(Self::Optimization),
            "editing" => Ok(Self::Editing),
            "final_review" => Ok(Self::FinalReview),
            "export" => Ok(Self::Export),
            _ => Err(PlatformError::InvalidChoice("stage")),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RequirementAnalysis => "requirement_analysis",
            Self::VisualSpec => "visual_spec",
            Self::StoryPlanning => "story_planning",
            Self::CharacterPlanning => "character_planning",
            Self::HumanApproval => "human_approval",
            Self::ImageGeneration => "image_generation",
            Self::VideoGeneration => "video_generation",
            Self::Review => "review",
            Self::Optimization => "optimization",
            Self::Editing => "editing",
            Self::FinalReview => "final_review",
            Self::Export => "export",
        }
    }
    pub fn display_label(&self) -> &'static str {
        match self {
            Self::RequirementAnalysis => "需求分析",
            Self::VisualSpec => "视觉规范",
            Self::StoryPlanning => "故事规划",
            Self::CharacterPlanning => "角色设计",
            Self::HumanApproval => "人工确认",
            Self::ImageGeneration => "图片生成",
            Self::VideoGeneration => "视频生成",
            Self::Review => "质量审核",
            Self::Optimization => "优化修订",
            Self::Editing => "剪辑合成",
            Self::FinalReview => "最终审核",
            Self::Export => "输出导出",
        }
    }

    /// 获取下一阶段（用于工作流推进）。
    pub fn next_stage(&self) -> Option<Self> {
        match self {
            Self::RequirementAnalysis => Some(Self::VisualSpec),
            Self::VisualSpec => Some(Self::StoryPlanning),
            Self::StoryPlanning => Some(Self::CharacterPlanning),
            Self::CharacterPlanning => Some(Self::HumanApproval),
            Self::HumanApproval => Some(Self::ImageGeneration),
            Self::ImageGeneration => Some(Self::VideoGeneration),
            Self::VideoGeneration => Some(Self::Review),
            Self::Review => Some(Self::Optimization),
            Self::Optimization => Some(Self::Editing),
            Self::Editing => Some(Self::FinalReview),
            Self::FinalReview => Some(Self::Export),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreativeRunRecord {
    pub id: String,
    pub project_id: String,
    pub user_goal: String,
    pub workflow_type: ProjectType,
    pub status: RunStatus,
    pub current_stage: WorkflowStage,
    pub budget_limit: Option<f64>,
    pub cost_estimate: f64,
    pub created_at: String,
    pub updated_at: String,
}

// ═══════════════════════════════════════════════════
// AgentStep
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

impl StepStatus {
    pub fn parse(v: &str) -> Result<Self, PlatformError> {
        match v {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "skipped" => Ok(Self::Skipped),
            _ => Err(PlatformError::InvalidChoice("stepStatus")),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStepRecord {
    pub id: String,
    pub run_id: String,
    pub agent_name: String,
    pub stage: WorkflowStage,
    pub status: StepStatus,
    pub input_snapshot_json: Option<String>,
    pub output_snapshot_json: Option<String>,
    pub error_message: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
}

// ═══════════════════════════════════════════════════
// RequirementSpec
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementSpecRecord {
    pub id: String,
    pub project_id: String,
    pub run_id: Option<String>,
    pub content_type: String,
    pub duration_seconds: Option<i32>,
    pub target_audience: Option<String>,
    pub platform: Option<String>,
    pub message: Option<String>,
    pub tone: Option<String>,
    pub must_have_json: Option<String>,
    pub must_not_have_json: Option<String>,
    pub raw_output: Option<String>,
    pub version: i64,
    pub created_at: String,
}

// ═══════════════════════════════════════════════════
// VisualSpec
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualSpecRecord {
    pub id: String,
    pub project_id: String,
    pub run_id: Option<String>,
    pub style: String,
    pub color_palette_json: Option<String>,
    pub composition: Option<String>,
    pub camera: Option<String>,
    pub lighting: Option<String>,
    pub mood: Option<String>,
    pub texture: Option<String>,
    pub negative_style: Option<String>,
    pub raw_output: Option<String>,
    pub version: i64,
    pub created_at: String,
}

// ═══════════════════════════════════════════════════
// ReviewReport
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewReportRecord {
    pub id: String,
    pub project_id: String,
    pub shot_id: Option<String>,
    pub asset_id: Option<String>,
    pub review_type: String,
    pub character_score: Option<f64>,
    pub style_score: Option<f64>,
    pub composition_score: Option<f64>,
    pub camera_score: Option<f64>,
    pub requirement_match_score: Option<f64>,
    pub overall_score: Option<f64>,
    pub issues_json: Option<String>,
    pub recommendation: Option<String>,
    pub created_at: String,
}

// ═══════════════════════════════════════════════════
// Error
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlatformError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} must be at most {max} characters")]
    TooLong { field: &'static str, max: usize },
    #[error("{0} is invalid")]
    InvalidChoice(&'static str),
    #[error("{0} is not a valid uuid")]
    InvalidUuid(&'static str),
}

impl PlatformError {
    pub fn user_message(&self) -> String {
        match self {
            Self::Required { field } => format!("{}不能为空。", field),
            Self::TooLong { field, max } => format!("{}不能超过 {} 个字符。", field, max),
            Self::InvalidChoice(f) => format!("{}无效。", f),
            Self::InvalidUuid(f) => format!("{}格式无效。", f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_type_roundtrip() {
        assert_eq!(
            ProjectType::parse("promo_video").unwrap(),
            ProjectType::PromoVideo
        );
        assert_eq!(ProjectType::PromoVideo.as_str(), "promo_video");
    }

    #[test]
    fn run_status_terminal() {
        assert!(RunStatus::Completed.is_terminal());
        assert!(!RunStatus::Running.is_terminal());
    }

    #[test]
    fn workflow_stage_display() {
        assert_eq!(
            WorkflowStage::RequirementAnalysis.display_label(),
            "需求分析"
        );
        assert_eq!(WorkflowStage::VideoGeneration.display_label(), "视频生成");
    }
}
