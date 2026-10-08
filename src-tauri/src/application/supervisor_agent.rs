#![allow(dead_code)]
//! Supervisor Agent：编排多 Agent 工作流。
//!
//! 接收用户需求，调度各专业 Agent，维护全局状态。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::application::sub_agent_runtime::{SubAgentRequest, SubAgentRuntime, SubAgentType};
use crate::domain::agents::{
    AgentOutput, AgentType, RequirementOutput, VisualSpecOutput, WorkflowPhase,
};
use crate::domain::task::TaskPriority;

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
    /// 子 Agent 运行时（可选）。接入后 `dispatch` 可真正驱动子 Agent。
    runtime: Option<Arc<SubAgentRuntime>>,
    /// 当前会话 ID（用于黑板消息隔离）。
    conversation_id: Mutex<Option<String>>,
    /// 工作区 ID。
    workspace_id: Mutex<String>,
}

impl SupervisorAgent {
    pub fn new(config: SupervisorConfig) -> Self {
        Self {
            config,
            phase: Mutex::new(WorkflowPhase::Idle),
            outputs: Mutex::new(Vec::new()),
            runtime: None,
            conversation_id: Mutex::new(None),
            workspace_id: Mutex::new("default".to_owned()),
        }
    }

    /// 接入子 Agent 运行时（启用真正的编排）。
    pub fn with_runtime(mut self, runtime: Arc<SubAgentRuntime>) -> Self {
        self.runtime = Some(runtime);
        self
    }

    /// 设置会话 ID 与工作区 ID。
    pub fn set_context(&self, conversation_id: Option<String>, workspace_id: String) {
        if let Ok(mut c) = self.conversation_id.lock() {
            *c = conversation_id;
        }
        if let Ok(mut w) = self.workspace_id.lock() {
            *w = workspace_id;
        }
    }

    fn conversation(&self) -> Option<String> {
        self.conversation_id.lock().ok().and_then(|c| c.clone())
    }

    fn workspace(&self) -> String {
        self.workspace_id
            .lock()
            .map(|w| w.clone())
            .unwrap_or_else(|_| "default".to_owned())
    }

    /// 派发一个子 Agent 任务、等待结果，并记录为 `AgentOutput`。
    ///
    /// 结果同时由 `SubAgentRuntime` 自动发布到黑板（topic = `sub_agent.result`）。
    pub fn dispatch(
        &self,
        agent_type: AgentType,
        prompt: String,
        timeout_secs: u64,
    ) -> Result<AgentOutput, String> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| "Supervisor 未接入 SubAgentRuntime".to_owned())?;

        let name = format!("{}-{}", agent_type.display_name(), short_uuid());
        let request = SubAgentRequest {
            name,
            agent_type: sub_agent_type_for(agent_type),
            prompt,
            priority: TaskPriority::default(),
            workspace_id: self.workspace(),
            conversation_id: self.conversation(),
            parameters: HashMap::new(),
            max_retries: 0,
            timeout_secs: Some(timeout_secs),
        };

        let result = runtime.spawn_and_wait(&request).map_err(|e| e.to_string())?;
        let raw_text = result.output.unwrap_or_default();
        let output = AgentOutput {
            agent_type,
            stage: self.current_phase().display_label().to_owned(),
            content: parse_json_or_string(&raw_text),
            raw_text,
            requires_approval: false,
        };
        self.add_output(output.clone());
        Ok(output)
    }

    /// 需求分析阶段：派发 Requirement 子 Agent 并推进阶段。
    pub fn run_requirement_phase(
        &self,
        user_goal: &str,
        timeout_secs: u64,
    ) -> Result<AgentOutput, String> {
        self.set_phase(WorkflowPhase::AnalyzingRequirement);
        let output = self.dispatch(
            AgentType::Requirement,
            Self::requirement_prompt(user_goal),
            timeout_secs,
        )?;
        self.advance();
        Ok(output)
    }

    /// 视觉规范阶段：派发 VisualDirector 子 Agent 并推进阶段。
    pub fn run_visual_spec_phase(
        &self,
        requirement: &RequirementOutput,
        timeout_secs: u64,
    ) -> Result<AgentOutput, String> {
        self.set_phase(WorkflowPhase::CreatingVisualSpec);
        let output = self.dispatch(
            AgentType::VisualDirector,
            Self::visual_spec_prompt(requirement),
            timeout_secs,
        )?;
        self.advance();
        Ok(output)
    }

    /// 剧本阶段：派发 Story 子 Agent 并推进阶段。
    pub fn run_story_phase(
        &self,
        requirement: &RequirementOutput,
        visual_spec: &VisualSpecOutput,
        timeout_secs: u64,
    ) -> Result<AgentOutput, String> {
        self.set_phase(WorkflowPhase::WritingStory);
        let output = self.dispatch(
            AgentType::Story,
            Self::story_prompt(requirement, visual_spec),
            timeout_secs,
        )?;
        self.advance();
        Ok(output)
    }

    /// 完整前置流水线：需求 → 视觉 → 剧本（串联子 Agent 编排）。
    pub fn run_preproduction_pipeline(
        &self,
        user_goal: &str,
        timeout_secs: u64,
    ) -> Result<Vec<AgentOutput>, String> {
        let requirement_output = self.run_requirement_phase(user_goal, timeout_secs)?;
        let requirement = parse_requirement(&requirement_output)
            .ok_or_else(|| "需求分析输出不是合法 JSON".to_owned())?;
        let visual_output = self.run_visual_spec_phase(&requirement, timeout_secs)?;
        let visual_spec = parse_visual_spec(&visual_output)
            .ok_or_else(|| "视觉规范输出不是合法 JSON".to_owned())?;
        let story_output = self.run_story_phase(&requirement, &visual_spec, timeout_secs)?;
        Ok(vec![requirement_output, visual_output, story_output])
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

/// AgentType → SubAgentType 映射（决定 TaskKind）。
fn sub_agent_type_for(agent_type: AgentType) -> SubAgentType {
    match agent_type {
        AgentType::ImageGeneration => SubAgentType::ImageGenerator,
        AgentType::VideoGeneration => SubAgentType::VideoGenerator,
        AgentType::Requirement | AgentType::VisualDirector | AgentType::Story => {
            SubAgentType::Analyzer
        }
        _ => SubAgentType::Generic,
    }
}

/// 取 UUID 前 8 位作为短标识。
fn short_uuid() -> String {
    uuid::Uuid::new_v4()
        .to_string()
        .split('-')
        .next()
        .unwrap_or("0")
        .to_owned()
}

/// 从 LLM 文本中解析 JSON；失败时回退为原始字符串值。
fn parse_json_or_string(text: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(extract_json_block(text))
        .unwrap_or_else(|_| serde_json::Value::String(text.to_owned()))
}

/// 提取文本中首个 `{` 到末个 `}` 的片段（容忍 ```json 代码块包裹）。
fn extract_json_block(text: &str) -> &str {
    match (text.find('{'), text.rfind('}')) {
        (Some(start), Some(end)) if end > start => &text[start..=end],
        _ => text,
    }
}

/// 从需求分析输出解析 `RequirementOutput`。
fn parse_requirement(output: &AgentOutput) -> Option<RequirementOutput> {
    serde_json::from_value(output.content.clone()).ok()
}

/// 从视觉规范输出解析 `VisualSpecOutput`。
fn parse_visual_spec(output: &AgentOutput) -> Option<VisualSpecOutput> {
    serde_json::from_value(output.content.clone()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::task::TaskStatus;
    use crate::ports::task_runtime::{TaskError, TaskHandle, TaskRequest, TaskRuntime};

    /// 桩 TaskRuntime：submit 即完成，输出为预设文本。
    struct StubTaskRuntime {
        output: String,
    }

    impl TaskRuntime for StubTaskRuntime {
        fn submit(&self, _request: &TaskRequest) -> Result<TaskHandle, TaskError> {
            let mut metadata = HashMap::new();
            metadata.insert(
                "output".to_owned(),
                serde_json::Value::String(self.output.clone()),
            );
            Ok(TaskHandle {
                task_id: "stub".to_owned(),
                status: TaskStatus::Completed,
                provider_id: None,
                model_name: None,
                remote_job_id: None,
                metadata,
            })
        }

        fn status(&self, _task_id: &str) -> Result<TaskStatus, TaskError> {
            Ok(TaskStatus::Completed)
        }

        fn cancel(&self, _task_id: &str) -> Result<bool, TaskError> {
            Ok(true)
        }
    }

    #[test]
    fn maps_agent_type_to_sub_agent_type() {
        assert!(matches!(
            sub_agent_type_for(AgentType::Requirement),
            SubAgentType::Analyzer
        ));
        assert!(matches!(
            sub_agent_type_for(AgentType::ImageGeneration),
            SubAgentType::ImageGenerator
        ));
        assert!(matches!(
            sub_agent_type_for(AgentType::VideoGeneration),
            SubAgentType::VideoGenerator
        ));
        assert!(matches!(
            sub_agent_type_for(AgentType::Editing),
            SubAgentType::Generic
        ));
    }

    #[test]
    fn extracts_json_from_code_fence() {
        let value = parse_json_or_string("好的：\n```json\n{\"a\":1}\n```\n以上");
        assert_eq!(value.get("a").and_then(|v| v.as_i64()), Some(1));
    }

    #[test]
    fn falls_back_to_string_when_not_json() {
        let value = parse_json_or_string("不是 JSON");
        assert_eq!(value.as_str(), Some("不是 JSON"));
    }

    #[test]
    fn dispatch_without_runtime_errors() {
        let supervisor = SupervisorAgent::new(SupervisorConfig::default());
        let err = supervisor
            .dispatch(AgentType::Requirement, "p".to_owned(), 1)
            .unwrap_err();
        assert!(err.contains("未接入"));
    }

    #[test]
    fn requirement_phase_records_output_and_advances() {
        let json = r#"{"contentType":"promo_video","durationSeconds":30,"targetAudience":"年轻人","platform":"抖音","message":"春节旅游","tone":"欢快","mustHave":[],"mustNotHave":[],"missingQuestions":[]}"#;
        let runtime = Arc::new(
            SubAgentRuntime::new(Arc::new(StubTaskRuntime {
                output: json.to_owned(),
            }))
            .with_polling(std::time::Duration::from_millis(1), 5),
        );
        let supervisor = SupervisorAgent::new(SupervisorConfig::default()).with_runtime(runtime);
        supervisor.set_context(Some("conv-1".to_owned()), "ws-1".to_owned());

        let output = supervisor
            .run_requirement_phase("做一支 30 秒春节旅游宣传片", 5)
            .unwrap();
        assert_eq!(output.agent_type, AgentType::Requirement);
        assert!(parse_requirement(&output).is_some());
        // 阶段推进：AnalyzingRequirement → CreatingVisualSpec
        assert_eq!(
            supervisor.current_phase(),
            WorkflowPhase::CreatingVisualSpec
        );
        assert_eq!(supervisor.outputs().len(), 1);
    }
}
