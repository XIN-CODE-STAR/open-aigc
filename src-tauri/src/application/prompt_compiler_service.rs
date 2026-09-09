//! PromptCompilerService — 提示词编译服务。
//!
//! 从 ScriptParser 中分离出的提示词生成逻辑。
//! 输入：语义模型（Shot + Scene + Project + CanvasContext）→ 输出：最终提示词。
//!
//! 设计原则：
//! - 与 ScriptParser 解耦（解析 ≠ 生成）
//! - 使用 CanvasContextService 获取相关上下文（不是全量画布 dump）
//! - 整合 StyleTokens、CharacterProfile 等创意状态
//! - 输出 CompiledPrompt 包含使用的上下文节点信息

use std::sync::{Arc, Mutex};

use crate::application::canvas_context_service::CanvasContextService;
use crate::application::error::AppError;
use crate::domain::canvas::ContextBudget;

// ──────────────────────────────────────────────────────────────────
// CompiledPrompt — 编译后的提示词
// ──────────────────────────────────────────────────────────────────

/// 编译后的提示词 — PromptCompilerService 的输出。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledPrompt {
    /// 最终提示词文本。
    pub prompt: String,
    /// 负面提示词。
    pub negative_prompt: String,
    /// 参与编译的上下文节点 ID。
    pub context_nodes_used: Vec<String>,
    /// 预估 token 数。
    pub estimated_tokens: usize,
}

// ──────────────────────────────────────────────────────────────────
// ShotInfo — 镜头信息（调用方提供）
// ──────────────────────────────────────────────────────────────────

/// 镜头信息 — 调用方提供的语义模型。
#[derive(Debug, Clone)]
pub struct ShotInfo {
    pub shot_id: String,
    pub description: String,
    /// 视觉意图 — 表达"我要拍什么"，不是最终 prompt。
    pub visual_intent: Option<String>,
    pub shot_type: Option<String>,
    pub camera_motion: Option<String>,
}

/// 场景信息。
#[derive(Debug, Clone)]
pub struct SceneInfo {
    pub scene_id: String,
    pub title: String,
    pub summary: Option<String>,
    pub location: Option<String>,
}

/// 项目信息。
#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub project_id: String,
    pub title: String,
    pub style: Option<String>,
}

// ──────────────────────────────────────────────────────────────────
// PromptCompilerService
// ──────────────────────────────────────────────────────────────────

/// 提示词编译服务。
///
/// 从语义模型（Shot + Scene + Project）和画布上下文中
/// 编译出最终的图片/视频生成提示词。
pub struct PromptCompilerService {
    canvas_context_service: Arc<CanvasContextService>,
}

impl PromptCompilerService {
    pub fn new(canvas_context_service: Arc<CanvasContextService>) -> Self {
        Self {
            canvas_context_service,
        }
    }

    /// 为指定镜头编译提示词。
    ///
    /// 整合信息：
    /// 1. Shot 自身的 prompt/description
    /// 2. Scene 的 location/summary
    /// 3. Project 的 style
    /// 4. 画布上下文中的相关节点（角色引用、前序镜头等）
    pub fn compile_for_shot(
        &self,
        canvas_id: &str,
        shot: &ShotInfo,
        scene: &SceneInfo,
        project: &ProjectInfo,
    ) -> Result<CompiledPrompt, AppError> {
        // Get relevant canvas context for this shot
        let budget = ContextBudget {
            max_nodes: 10,
            max_tokens: 2000,
        };
        let prompt_context =
            self.canvas_context_service
                .get_context_for_prompt(canvas_id, &shot.shot_id, budget)?;

        // Build the final prompt
        let mut parts: Vec<String> = Vec::new();

        // 1. Project style prefix
        if let Some(ref style) = project.style {
            if !style.is_empty() {
                parts.push(style.clone());
            }
        }

        // 2. Scene context
        if let Some(ref location) = scene.location {
            parts.push(format!("{} setting", location));
        }

        // 3. Character references from canvas context
        for char_desc in &prompt_context.character_descriptions {
            parts.push(char_desc.clone());
        }

        // 4. Shot visual intent (main content — not the final prompt yet)
        if let Some(ref visual_intent) = shot.visual_intent {
            parts.push(visual_intent.clone());
        } else {
            parts.push(shot.description.clone());
        }

        // 5. Shot type framing
        if let Some(ref shot_type) = shot.shot_type {
            let framing = match shot_type.as_str() {
                "wide" => "wide establishing shot, cinematic lighting",
                "close-up" => "close-up shot, shallow depth of field",
                "extreme-close-up" => "extreme close-up, macro detail",
                "aerial" => "aerial view, bird's eye perspective",
                _ => "medium shot, balanced composition",
            };
            parts.push(framing.to_owned());
        }

        // 6. Camera motion
        if let Some(ref motion) = shot.camera_motion {
            if motion != "static" {
                let motion_desc = match motion.as_str() {
                    "dolly-in" => "dolly in, moving closer",
                    "dolly-out" => "dolly out, moving away",
                    "tracking" => "tracking shot, following subject",
                    "orbit" => "orbit shot, rotating around subject",
                    "pan" => "panning shot, horizontal sweep",
                    "crane-up" => "crane shot, rising up",
                    "crane-down" => "crane shot, descending",
                    _ => motion.as_str(),
                };
                parts.push(motion_desc.to_owned());
            }
        }

        // 7. Related artifact prompts (for consistency with previous shots)
        for related_prompt in prompt_context.related_artifact_prompts.iter().take(2) {
            // Only add style hints, not full prompts
            if let Some(style_hint) = Self::extract_style_hint(related_prompt) {
                parts.push(style_hint);
            }
        }

        let prompt = parts.join(", ");
        let negative_prompt = "low quality, blurry, distorted, watermark, text".to_owned();

        let context_nodes_used: Vec<String> = prompt_context
            .contextual_nodes
            .iter()
            .map(|cn| cn.node.id.clone())
            .collect();

        let estimated_tokens = prompt.split_whitespace().count() * 2 // rough estimate
            + prompt_context.estimated_tokens;

        Ok(CompiledPrompt {
            prompt,
            negative_prompt,
            context_nodes_used,
            estimated_tokens,
        })
    }

    /// 从相关镜头的 prompt 中提取风格提示词（取最后的 shot label 部分）。
    fn extract_style_hint(prompt: &str) -> Option<String> {
        // 从包含镜头标签（shot）的片段起截取到结尾作为风格提示。
        let parts: Vec<&str> = prompt.split(", ").collect();
        if let Some(pos) = parts.iter().position(|part| part.contains("shot")) {
            return Some(parts[pos..].join(", "));
        }
        // 无镜头标签时，退化为末段风格词。
        let last = parts.last().copied()?;
        if last.contains("lighting") {
            return Some(last.to_owned());
        }
        None
    }
}

// ──────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_style_hint_with_shot_label() {
        let hint = PromptCompilerService::extract_style_hint(
            "anime style, night city, wide establishing shot, cinematic lighting",
        );
        assert_eq!(
            hint,
            Some("wide establishing shot, cinematic lighting".to_owned())
        );
    }

    #[test]
    fn extract_style_hint_no_shot_label() {
        let hint = PromptCompilerService::extract_style_hint("just some text");
        assert!(hint.is_none());
    }
}
