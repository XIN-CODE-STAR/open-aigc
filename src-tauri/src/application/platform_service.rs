#![allow(dead_code)]
//! 平台服务：管理项目、创意运行、Agent 编排。
//!
//! 按照架构文档第 4、6、7 节实现：
//! - 项目 CRUD
//! - 创意运行生命周期
//! - Agent 调度和工作流推进

use std::sync::Mutex;

use crate::application::error::AppError;
use crate::domain::agents::{RequirementOutput, VisualSpecOutput};
use crate::domain::platform::{
    CreativeRunRecord, ProjectRecord, ProjectStatus, ProjectType, RunStatus, WorkflowStage,
};

/// 平台服务配置。
pub struct PlatformServiceConfig {
    pub max_iterations_per_run: u8,
    pub auto_approve_drafts: bool,
}

impl Default for PlatformServiceConfig {
    fn default() -> Self {
        Self {
            max_iterations_per_run: 10,
            auto_approve_drafts: true,
        }
    }
}

/// 平台服务：管理整个创作流水线。
pub struct PlatformService {
    config: PlatformServiceConfig,
    // 在实际实现中，这里会持有 repository 的 Mutex
    // 为简化，先用内存状态
    projects: Mutex<Vec<ProjectRecord>>,
    runs: Mutex<Vec<CreativeRunRecord>>,
}

impl PlatformService {
    pub fn new(config: PlatformServiceConfig) -> Self {
        Self {
            config,
            projects: Mutex::new(Vec::new()),
            runs: Mutex::new(Vec::new()),
        }
    }

    /// 创建新项目。
    pub fn create_project(
        &self,
        workspace_id: &str,
        title: &str,
        project_type: ProjectType,
        description: Option<&str>,
        target_duration: Option<i32>,
        target_platform: Option<&str>,
    ) -> Result<ProjectRecord, AppError> {
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "unknown".to_owned());
        let project = ProjectRecord {
            id: uuid::Uuid::new_v4().to_string(),
            workspace_id: workspace_id.to_owned(),
            owner_teacher_id: None,
            title: title.to_owned(),
            description: description.map(|s| s.to_owned()),
            project_type,
            status: ProjectStatus::Draft,
            target_duration_seconds: target_duration,
            target_platform: target_platform.map(|s| s.to_owned()),
            created_at: now.clone(),
            updated_at: now,
        };

        let mut projects = self
            .projects
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        projects.push(project.clone());
        Ok(project)
    }

    /// 列出项目。
    pub fn list_projects(&self, _workspace_id: &str) -> Result<Vec<ProjectRecord>, AppError> {
        let projects = self
            .projects
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        Ok(projects.clone())
    }

    /// 创建创意运行。
    pub fn create_run(
        &self,
        project_id: &str,
        user_goal: &str,
        workflow_type: ProjectType,
        budget_limit: Option<f64>,
    ) -> Result<CreativeRunRecord, AppError> {
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "unknown".to_owned());
        let run = CreativeRunRecord {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: project_id.to_owned(),
            user_goal: user_goal.to_owned(),
            workflow_type,
            status: RunStatus::Pending,
            current_stage: WorkflowStage::RequirementAnalysis,
            budget_limit,
            cost_estimate: 0.0,
            created_at: now.clone(),
            updated_at: now,
        };

        let mut runs = self.runs.lock().map_err(|_| AppError::StateUnavailable)?;
        runs.push(run.clone());
        Ok(run)
    }

    /// 列出创意运行。
    pub fn list_runs(&self, project_id: &str) -> Result<Vec<CreativeRunRecord>, AppError> {
        let runs = self.runs.lock().map_err(|_| AppError::StateUnavailable)?;
        Ok(runs
            .iter()
            .filter(|r| r.project_id == project_id)
            .cloned()
            .collect())
    }

    /// 推进创意运行到下一阶段。
    pub fn advance_run(&self, run_id: &str) -> Result<CreativeRunRecord, AppError> {
        let mut runs = self.runs.lock().map_err(|_| AppError::StateUnavailable)?;
        let run = runs
            .iter_mut()
            .find(|r| r.id == run_id)
            .ok_or_else(|| AppError::PlatformRunNotFound(run_id.to_owned()))?;

        if let Some(next_stage) = run.current_stage.next_stage() {
            run.current_stage = next_stage;
            run.status = if run.current_stage == WorkflowStage::Export {
                RunStatus::Completed
            } else {
                RunStatus::Running
            };
            run.updated_at = time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "unknown".to_owned());
        }

        Ok(run.clone())
    }

    /// 标记运行为等待审批。
    pub fn request_approval(&self, run_id: &str) -> Result<CreativeRunRecord, AppError> {
        let mut runs = self.runs.lock().map_err(|_| AppError::StateUnavailable)?;
        let run = runs
            .iter_mut()
            .find(|r| r.id == run_id)
            .ok_or_else(|| AppError::PlatformRunNotFound(run_id.to_owned()))?;

        run.status = RunStatus::WaitingApproval;
        run.updated_at = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "unknown".to_owned());
        Ok(run.clone())
    }

    /// 批准创意运行。
    pub fn approve_run(&self, run_id: &str) -> Result<CreativeRunRecord, AppError> {
        let mut runs = self.runs.lock().map_err(|_| AppError::StateUnavailable)?;
        let run = runs
            .iter_mut()
            .find(|r| r.id == run_id)
            .ok_or_else(|| AppError::PlatformRunNotFound(run_id.to_owned()))?;

        run.status = RunStatus::Running;
        if let Some(next) = run.current_stage.next_stage() {
            run.current_stage = next;
        }
        run.updated_at = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "unknown".to_owned());
        Ok(run.clone())
    }

    /// 构建需求分析提示词。
    pub fn build_requirement_prompt(user_goal: &str) -> String {
        format!(
            "你是一位专业的影视需求分析师。用户的需求是：\n\n\"{user_goal}\"\n\n\
             请分析这个需求，提取以下信息并以 JSON 格式输出：\n\
             - content_type: 内容类型\n\
             - duration_seconds: 建议时长（秒）\n\
             - target_audience: 目标受众\n\
             - platform: 目标平台\n\
             - message: 核心信息/主题\n\
             - tone: 情感基调\n\
             - must_have: 必须包含的元素（数组）\n\
             - must_not_have: 不能包含的元素（数组）\n\
             - missing_questions: 需要用户补充的信息（数组）\n\n\
             用中文回复。"
        )
    }

    /// 构建视觉规范提示词。
    pub fn build_visual_spec_prompt(requirement: &RequirementOutput) -> String {
        format!(
            "你是一位专业的视觉总监。根据以下需求创建视觉规范：\n\n\
             内容类型：{}\n目标受众：{}\n核心信息：{}\n情感基调：{}\n\n\
             请输出视觉规范（JSON）：\n\
             - style / color_palette / composition / camera / lighting / mood / texture / negative_style\n\n\
             用中文回复。",
            requirement.content_type, requirement.target_audience, requirement.message, requirement.tone
        )
    }

    /// 构建故事规划提示词。
    pub fn build_story_prompt(
        requirement: &RequirementOutput,
        visual: &VisualSpecOutput,
    ) -> String {
        format!(
            "你是一位专业的影视编剧。根据需求和视觉规范创作剧本和分镜：\n\n\
             需求：{}\n时长：{}秒\n视觉风格：{}\n色彩：{:?}\n\n\
             输出 JSON：title / logline / synopsis / shots[]\n\
             每个镜头：shot_id / duration_seconds / scene / camera / movement / emotion / purpose / voiceover / image_prompt / video_prompt\n\n\
             镜头总时长应接近 {} 秒。用中文回复。",
            requirement.message, requirement.duration_seconds, visual.style, visual.color_palette, requirement.duration_seconds
        )
    }
}
