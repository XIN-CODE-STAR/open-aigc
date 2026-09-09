#![allow(dead_code)]
//! Creative Director Agent：创作决策层。
//!
//! 在规划阶段之前运行，分析用户创作意图，确定视觉方向，
//! 更新 CreativeState，并将创作上下文注入规划提示词。
//!
//! 职责：
//! - 理解创作目的（海报？视频？漫画？品牌？）
//! - 判断目标用户
//! - 确定视觉方向
//! - 制定创作策略
//! - 更新 CreativeState 中的决策记录

use crate::{
    application::{creative_state_service::CreativeStateService, error::AppError},
    domain::{
        agent::{ChatMessage, ChatRequest},
        creative_state::{CreativeStateRecord, DecisionRecord, StyleTokens},
    },
    ports::agent_llm::AgentLlm,
};

/// Creative Director 分析结果。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreativeBrief {
    /// 项目类型判断。
    pub project_type: String,
    /// 目标受众。
    pub audience: String,
    /// 投放平台。
    pub platform: String,
    /// 视觉方向建议。
    pub visual_direction: String,
    /// 创作策略摘要。
    pub strategy: String,
    /// 建议的风格 Token（如果 LLM 给出了明确建议）。
    pub suggested_style: Option<StyleTokens>,
}

/// Creative Director 服务。
pub struct CreativeDirectorService {
    creative_state_service: std::sync::Arc<CreativeStateService>,
}

impl CreativeDirectorService {
    pub fn new(creative_state_service: std::sync::Arc<CreativeStateService>) -> Self {
        Self {
            creative_state_service,
        }
    }

    /// 获取工作区的创作上下文（用于注入规划提示词）。
    ///
    /// 如果工作区已有 CreativeState，返回其 prompt context；否则返回 None。
    pub fn get_creative_context(&self, workspace_id: &str) -> Result<Option<String>, AppError> {
        self.creative_state_service.get_prompt_context(workspace_id)
    }

    /// 分析用户创作请求，生成 Creative Brief 并更新 CreativeState。
    ///
    /// 调用 LLM 分析用户意图，提取项目类型、受众、平台、视觉方向，
    /// 然后将决策写入 CreativeState。
    pub fn analyze_and_direct(
        &self,
        workspace_id: &str,
        user_message: &str,
        model_name: &str,
        llm: &mut dyn AgentLlm,
    ) -> Result<CreativeBrief, AppError> {
        // 加载现有状态
        let existing_state = self.creative_state_service.get_by_workspace(workspace_id)?;

        // 构建分析提示词
        let analysis_prompt = build_director_analysis_prompt(user_message, existing_state.as_ref());

        let chat_messages = vec![
            ChatMessage::system(analysis_prompt),
            ChatMessage::user(user_message.to_owned()),
        ];
        let request = ChatRequest::new(model_name.to_owned(), chat_messages, Vec::new());

        let response = llm.chat(&request).map_err(|e| {
            AppError::AgentLlmError(format!("creative director analysis failed: {e}"))
        })?;

        let content = response.content.unwrap_or_default();

        // 解析 LLM 响应为 CreativeBrief
        let brief = parse_creative_brief(&content);

        // 更新 CreativeState
        self.apply_brief_to_state(workspace_id, &brief, user_message)?;

        Ok(brief)
    }

    /// 将 Creative Brief 的决策写入 CreativeState。
    fn apply_brief_to_state(
        &self,
        workspace_id: &str,
        brief: &CreativeBrief,
        user_message: &str,
    ) -> Result<(), AppError> {
        // 记录决策
        let decision = DecisionRecord {
            id: uuid::Uuid::new_v4().to_string(),
            phase: "requirement".to_owned(),
            decision: format!(
                "项目类型：{}；受众：{}；平台：{}；视觉方向：{}",
                brief.project_type, brief.audience, brief.platform, brief.visual_direction
            ),
            reason: format!("基于用户请求分析：{}", truncate(user_message, 100)),
            alternatives_considered: Vec::new(),
            decided_at: now_rfc3339(),
        };
        self.creative_state_service
            .save_decision(workspace_id, decision)?;

        // 如果有风格建议，更新 StyleTokens
        if let Some(style) = &brief.suggested_style {
            if !style.is_empty() {
                self.creative_state_service
                    .update_style_tokens(workspace_id, style.clone())?;
            }
        }

        Ok(())
    }

    /// 构建增强的规划提示词（注入创作上下文）。
    pub fn build_enriched_planning_context(
        &self,
        workspace_id: &str,
        memory_context: Option<&str>,
    ) -> Result<String, AppError> {
        let mut context = String::new();

        // 注入创作状态
        if let Some(creative_ctx) = self.get_creative_context(workspace_id)? {
            context.push_str(&creative_ctx);
            context.push_str("\n\n");
        }

        // 注入记忆
        if let Some(mem) = memory_context {
            if !mem.is_empty() {
                context.push_str("[相关记忆]\n");
                context.push_str(mem);
                context.push_str("\n\n");
            }
        }

        Ok(context)
    }
}

// ─── Prompt 构建 ───

fn build_director_analysis_prompt(
    _user_message: &str,
    existing_state: Option<&CreativeStateRecord>,
) -> String {
    let mut prompt = String::from(
        "你是一位资深创意总监（Creative Director），负责分析创作需求并制定创作策略。\n\n\
         请分析用户的创作请求，输出以下结构化信息（用 JSON 格式）：\n\n\
         ```json\n\
         {\n\
         \"projectType\": \"项目类型（poster/video/manga/brand/illustration/social_media）\",\n\
         \"audience\": \"目标受众描述\",\n\
         \"platform\": \"投放平台（wechat/douyin/xiaohongshu/print/web/general）\",\n\
         \"visualDirection\": \"视觉方向建议（一句话）\",\n\
         \"strategy\": \"创作策略（2-3句话）\",\n\
         \"style\": {\n\
         \"style\": \"整体风格\",\n\
         \"colorMood\": \"色彩情绪\",\n\
         \"composition\": \"构图建议\",\n\
         \"lighting\": \"光影建议\"\n\
         }\n\
         }\n\
         ```\n\n\
         注意：\n\
         - 只输出 JSON，不要其他内容\n\
         - 如果用户请求不明确，根据上下文做合理推断\n\
         - style 字段中不确定的留空字符串\n",
    );

    if let Some(state) = existing_state {
        prompt.push_str("\n[当前项目创作状态]\n");
        prompt.push_str(&state.to_prompt_context());
        prompt.push_str("\n请基于已有状态进行增量分析，不要推翻已确定的决策。\n");
    }

    prompt
}

/// 解析 LLM 响应为 CreativeBrief。
fn parse_creative_brief(content: &str) -> CreativeBrief {
    // 尝试从 JSON 代码块中提取
    let json_str = extract_json_block(content);

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json_str) {
        let style = value
            .get("style")
            .and_then(|s| serde_json::from_value::<StyleTokens>(s.clone()).ok());

        CreativeBrief {
            project_type: value
                .get("projectType")
                .and_then(|v| v.as_str())
                .unwrap_or("general")
                .to_owned(),
            audience: value
                .get("audience")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned(),
            platform: value
                .get("platform")
                .and_then(|v| v.as_str())
                .unwrap_or("general")
                .to_owned(),
            visual_direction: value
                .get("visualDirection")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned(),
            strategy: value
                .get("strategy")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned(),
            suggested_style: style,
        }
    } else {
        // 解析失败，返回默认 brief
        CreativeBrief {
            project_type: "general".to_owned(),
            audience: String::new(),
            platform: "general".to_owned(),
            visual_direction: String::new(),
            strategy: content.chars().take(200).collect(),
            suggested_style: None,
        }
    }
}

/// 从文本中提取 JSON 代码块。
fn extract_json_block(content: &str) -> String {
    // 尝试找 ```json ... ``` 块
    if let Some(start) = content.find("```json") {
        let after_marker = &content[start + 7..];
        if let Some(end) = after_marker.find("```") {
            return after_marker[..end].trim().to_owned();
        }
    }
    // 尝试找 ``` ... ``` 块
    if let Some(start) = content.find("```") {
        let after_marker = &content[start + 3..];
        if let Some(end) = after_marker.find("```") {
            let block = after_marker[..end].trim();
            if block.starts_with('{') {
                return block.to_owned();
            }
        }
    }
    // 尝试找裸 JSON
    if let Some(start) = content.find('{') {
        if let Some(end) = content.rfind('}') {
            if end > start {
                return content[start..=end].to_owned();
            }
        }
    }
    content.to_owned()
}

fn truncate(s: &str, max: usize) -> String {
    // 按字符截断：中文等多字节文本按字节切分会 panic（不在字符边界上）。
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}...")
    }
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_handles_multibyte_text() {
        // 回归：中文文本按字节切分会 panic（不在字符边界上），
        // 曾导致整个 agent 任务以 "本地任务未能完成" 失败。
        // 旧实现按 len()（字节）判断后切 &s[..100]：60 个汉字 = 180 字节即触发 panic。
        let bytes_overflow = "张".repeat(60); // 60 字符 / 180 字节
        assert_eq!(truncate(&bytes_overflow, 100), bytes_overflow);

        let real_truncate = "张".repeat(120); // 120 字符 / 360 字节
        let truncated = truncate(&real_truncate, 100);
        assert_eq!(truncated.chars().count(), 103); // 100 字符 + "..."
        assert!(truncated.ends_with("..."));
        // 短文本原样返回
        assert_eq!(truncate("去除二维码", 100), "去除二维码");
    }
}
