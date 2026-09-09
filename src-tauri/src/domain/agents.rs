#![allow(dead_code)]
//! 多 Agent 系统定义。
//!
//! 按照 2026-07-20 架构文档第6节实现：
//! - Supervisor Agent：编排器
//! - Requirement Agent：需求分析
//! - Visual Director Agent：视觉规范
//! - Story Agent：剧本和分镜

use serde::{Deserialize, Serialize};

/// Agent 类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    Supervisor,
    Requirement,
    VisualDirector,
    Story,
    Character,
    ImageGeneration,
    VideoGeneration,
    VoiceMusic,
    Editing,
    Review,
    Optimization,
}

impl AgentType {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Supervisor => "导演",
            Self::Requirement => "需求分析师",
            Self::VisualDirector => "视觉总监",
            Self::Story => "编剧",
            Self::Character => "角色设计师",
            Self::ImageGeneration => "图片生成师",
            Self::VideoGeneration => "视频生成师",
            Self::VoiceMusic => "配音/音乐师",
            Self::Editing => "剪辑师",
            Self::Review => "审片师",
            Self::Optimization => "优化师",
        }
    }
}

/// Agent 运行上下文。
#[derive(Debug, Clone)]
pub struct AgentContext {
    pub run_id: String,
    pub project_id: String,
    pub user_goal: String,
    pub current_stage: String,
    pub previous_outputs: Vec<AgentOutput>,
}

/// Agent 输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentOutput {
    pub agent_type: AgentType,
    pub stage: String,
    pub content: serde_json::Value,
    pub raw_text: String,
    pub requires_approval: bool,
}

/// 需求分析输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementOutput {
    pub content_type: String,
    pub duration_seconds: i32,
    pub target_audience: String,
    pub platform: String,
    pub message: String,
    pub tone: String,
    pub must_have: Vec<String>,
    pub must_not_have: Vec<String>,
    pub missing_questions: Vec<String>,
}

/// 视觉规范输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualSpecOutput {
    pub style: String,
    pub color_palette: Vec<String>,
    pub composition: String,
    pub camera: String,
    pub lighting: String,
    pub mood: String,
    pub texture: String,
    pub negative_style: String,
}

/// 镜头输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShotOutput {
    pub shot_id: String,
    pub duration_seconds: f64,
    pub scene: String,
    pub camera: String,
    pub movement: String,
    pub emotion: String,
    pub purpose: String,
    pub voiceover: Option<String>,
    pub image_prompt: Option<String>,
    pub video_prompt: Option<String>,
}

/// 剧本输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryOutput {
    pub title: String,
    pub logline: String,
    pub synopsis: String,
    pub shots: Vec<ShotOutput>,
    pub total_duration_seconds: f64,
}

/// 审片报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewOutput {
    pub character_score: f64,
    pub style_score: f64,
    pub composition_score: f64,
    pub camera_score: f64,
    pub requirement_match_score: f64,
    pub overall_score: f64,
    pub issues: Vec<ReviewIssue>,
    pub recommendation: String,
}

/// 审片问题。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewIssue {
    pub issue_type: String,
    pub severity: String,
    pub message: String,
}

/// 工作流阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowPhase {
    Idle,
    AnalyzingRequirement,
    CreatingVisualSpec,
    WritingStory,
    DesigningCharacters,
    AwaitingApproval,
    GeneratingImages,
    GeneratingVideos,
    Reviewing,
    Optimizing,
    Editing,
    Exporting,
    Completed,
    Failed,
}

impl WorkflowPhase {
    pub fn display_label(&self) -> &'static str {
        match self {
            Self::Idle => "就绪",
            Self::AnalyzingRequirement => "分析需求…",
            Self::CreatingVisualSpec => "创建视觉规范…",
            Self::WritingStory => "编写剧本…",
            Self::DesigningCharacters => "设计角色…",
            Self::AwaitingApproval => "等待确认",
            Self::GeneratingImages => "生成图片…",
            Self::GeneratingVideos => "生成视频…",
            Self::Reviewing => "质量审核…",
            Self::Optimizing => "优化修订…",
            Self::Editing => "剪辑合成…",
            Self::Exporting => "导出成品…",
            Self::Completed => "已完成",
            Self::Failed => "失败",
        }
    }

    pub fn next_phase(&self) -> Option<Self> {
        match self {
            Self::AnalyzingRequirement => Some(Self::CreatingVisualSpec),
            Self::CreatingVisualSpec => Some(Self::WritingStory),
            Self::WritingStory => Some(Self::DesigningCharacters),
            Self::DesigningCharacters => Some(Self::AwaitingApproval),
            Self::AwaitingApproval => Some(Self::GeneratingImages),
            Self::GeneratingImages => Some(Self::GeneratingVideos),
            Self::GeneratingVideos => Some(Self::Reviewing),
            Self::Reviewing => Some(Self::Optimizing),
            Self::Optimizing => Some(Self::Editing),
            Self::Editing => Some(Self::Exporting),
            Self::Exporting => Some(Self::Completed),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_phase_display() {
        assert_eq!(
            WorkflowPhase::AnalyzingRequirement.display_label(),
            "分析需求…"
        );
        assert_eq!(WorkflowPhase::Completed.display_label(), "已完成");
    }

    #[test]
    fn workflow_phase_next() {
        assert_eq!(
            WorkflowPhase::AnalyzingRequirement.next_phase(),
            Some(WorkflowPhase::CreatingVisualSpec)
        );
        assert_eq!(WorkflowPhase::Completed.next_phase(), None);
    }

    #[test]
    fn agent_type_display() {
        assert_eq!(AgentType::Supervisor.display_name(), "导演");
        assert_eq!(AgentType::Story.display_name(), "编剧");
    }
}
