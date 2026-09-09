#![allow(dead_code)]
//! Supervisor Agent：编排多 Agent 工作流。
//!
//! 接收用户需求，调度各专业 Agent，维护全局状态。

use std::sync::Mutex;

use crate::domain::agents::{AgentOutput, RequirementOutput, VisualSpecOutput, WorkflowPhase};

/// Supervisor Agent 配置。
pub struct SupervisorConfig {
    pub max_iterations: u8,
    pub auto_approve_drafts: bool,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            auto_approve_drafts: true,
        }
    }
}

/// Supervisor Agent 状态。
pub struct SupervisorAgent {
    config: SupervisorConfig,
    phase: Mutex<WorkflowPhase>,
    outputs: Mutex<Vec<AgentOutput>>,
}

impl SupervisorAgent {
    pub fn new(config: SupervisorConfig) -> Self {
        Self {
            config,
            phase: Mutex::new(WorkflowPhase::Idle),
            outputs: Mutex::new(Vec::new()),
        }
    }

    /// 获取当前阶段。
    pub fn current_phase(&self) -> WorkflowPhase {
        self.phase.lock().map(|p| *p).unwrap_or(WorkflowPhase::Idle)
    }

    /// 设置阶段。
    pub fn set_phase(&self, phase: WorkflowPhase) {
        if let Ok(mut p) = self.phase.lock() {
            *p = phase;
        }
    }

    /// 添加 Agent 输出。
    pub fn add_output(&self, output: AgentOutput) {
        if let Ok(mut outputs) = self.outputs.lock() {
            outputs.push(output);
        }
    }

    /// 获取所有输出。
    pub fn outputs(&self) -> Vec<AgentOutput> {
        self.outputs.lock().map(|o| o.clone()).unwrap_or_default()
    }

    /// 清空输出。
    pub fn clear_outputs(&self) {
        if let Ok(mut outputs) = self.outputs.lock() {
            outputs.clear();
        }
    }

    /// 判断当前阶段是否需要人工确认。
    pub fn requires_approval(&self) -> bool {
        matches!(self.current_phase(), WorkflowPhase::AwaitingApproval)
    }

    /// 推进到下一阶段。
    pub fn advance(&self) -> WorkflowPhase {
        let current = self.current_phase();
        if let Some(next) = current.next_phase() {
            self.set_phase(next);
            next
        } else {
            current
        }
    }

    /// 重置到初始状态。
    pub fn reset(&self) {
        self.set_phase(WorkflowPhase::Idle);
        self.clear_outputs();
    }

    /// 构建需求分析的系统提示词。
    pub fn requirement_prompt(user_goal: &str) -> String {
        format!(
            "你是一位专业的影视需求分析师。用户的需求是：\n\n\"{user_goal}\"\n\n\
             请分析这个需求，提取以下信息并以 JSON 格式输出：\n\
             - content_type: 内容类型（promo_video/short_film/product_video/documentary/social_media/educational）\n\
             - duration_seconds: 建议时长（秒）\n\
             - target_audience: 目标受众\n\
             - platform: 目标平台\n\
             - message: 核心信息/主题\n\
             - tone: 情感基调\n\
             - must_have: 必须包含的元素（数组）\n\
             - must_not_have: 不能包含的元素（数组）\n\
             - missing_questions: 需要用户补充的信息（数组，如果没有则为空）\n\n\
             用中文回复，JSON 用双引号。"
        )
    }

    /// 构建视觉规范的系统提示词。
    pub fn visual_spec_prompt(requirement: &RequirementOutput) -> String {
        format!(
            "你是一位专业的视觉总监。根据以下需求创建视觉规范：\n\n\
             内容类型：{}\n\
             目标受众：{}\n\
             核心信息：{}\n\
             情感基调：{}\n\
             风格参考：{}\n\n\
             请输出以下视觉规范（JSON 格式）：\n\
             - style: 整体视觉风格\n\
             - color_palette: 色彩方案（数组）\n\
             - composition: 构图规则\n\
             - camera: 镜头风格\n\
             - lighting: 光线风格\n\
             - mood: 情绪氛围\n\
             - texture: 材质纹理\n\
             - negative_style: 需要避免的风格\n\n\
             用中文回复。",
            requirement.content_type,
            requirement.target_audience,
            requirement.message,
            requirement.tone,
            requirement.platform,
        )
    }

    /// 构建故事规划的系统提示词。
    pub fn story_prompt(requirement: &RequirementOutput, visual_spec: &VisualSpecOutput) -> String {
        format!(
            "你是一位专业的影视编剧。根据以下需求和视觉规范创作剧本和分镜：\n\n\
             需求：{}\n\
             时长：{}秒\n\
             视觉风格：{}\n\
             色彩：{:?}\n\n\
             请输出（JSON 格式）：\n\
             - title: 片名\n\
             - logline: 一句话简介\n\
             - synopsis: 故事梗概\n\
             - shots: 镜头列表，每个镜头包含：\n\
               - shot_id: 镜头编号\n\
               - duration_seconds: 时长\n\
               - scene: 场景描述\n\
               - camera: 镜头类型\n\
               - movement: 运镜方式\n\
               - emotion: 情绪\n\
               - purpose: 镜头目的\n\
               - voiceover: 旁白（可选）\n\
               - image_prompt: 图片生成提示词\n\
               - video_prompt: 视频生成提示词\n\n\
             镜头总时长应接近 {} 秒。用中文回复。",
            requirement.message,
            requirement.duration_seconds,
            visual_spec.style,
            visual_spec.color_palette,
            requirement.duration_seconds,
        )
    }
}
