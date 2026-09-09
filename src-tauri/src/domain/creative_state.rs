#![allow(dead_code)]
//! 创作状态空间（Creative State）。
//!
//! 项目级持久化创作上下文，所有 Agent 执行时读取，生成后更新。
//! 让 Agent 从"任务执行者"变成"创作伙伴"——记住项目目标、风格、角色、世界观。

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─── 验证错误 ───

#[derive(Debug, Error)]
pub enum CreativeStateValidationError {
    #[error("field '{field}' is required")]
    Required { field: String },
    #[error("field '{field}' exceeds max length {max}")]
    TooLong { field: String, max: usize },
    #[error("invalid choice for field '{field}'")]
    InvalidChoice { field: String },
}

// ─── Style Tokens ───

/// 结构化风格描述，从参考图分析或用户手动设定。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StyleTokens {
    /// 整体风格（"children illustration" / "cyberpunk" / "minimalist"）。
    #[serde(default)]
    pub style: String,
    /// 线条（"soft hand drawn" / "sharp vector" / "ink wash"）。
    #[serde(default)]
    pub line: String,
    /// 色板（hex 数组）。
    #[serde(default)]
    pub color_palette: Vec<String>,
    /// 色彩情绪（"warm" / "cool" / "neutral" / "vibrant"）。
    #[serde(default)]
    pub color_mood: String,
    /// 角色比例（"four head" / "realistic" / "chibi"）。
    #[serde(default)]
    pub character_proportion: String,
    /// 构图（"center focus" / "rule of thirds" / "symmetric"）。
    #[serde(default)]
    pub composition: String,
    /// 光影（"warm" / "dramatic" / "flat" / "backlit"）。
    #[serde(default)]
    pub lighting: String,
    /// 材质（"matte" / "glossy" / "grain" / "watercolor"）。
    #[serde(default)]
    pub texture: String,
    /// 明确不要的风格。
    #[serde(default)]
    pub negative_style: String,
}

impl StyleTokens {
    /// 是否为空（所有字段均为默认值）。
    pub fn is_empty(&self) -> bool {
        self.style.is_empty()
            && self.line.is_empty()
            && self.color_palette.is_empty()
            && self.color_mood.is_empty()
            && self.composition.is_empty()
            && self.lighting.is_empty()
    }

    /// 生成用于 Prompt 注入的风格描述文本。
    pub fn to_prompt_fragment(&self) -> String {
        let mut parts = Vec::new();
        if !self.style.is_empty() {
            parts.push(format!("风格：{}", self.style));
        }
        if !self.line.is_empty() {
            parts.push(format!("线条：{}", self.line));
        }
        if !self.color_mood.is_empty() {
            parts.push(format!("色彩：{}", self.color_mood));
        }
        if !self.color_palette.is_empty() {
            parts.push(format!("色板：{}", self.color_palette.join(", ")));
        }
        if !self.composition.is_empty() {
            parts.push(format!("构图：{}", self.composition));
        }
        if !self.lighting.is_empty() {
            parts.push(format!("光影：{}", self.lighting));
        }
        if !self.texture.is_empty() {
            parts.push(format!("材质：{}", self.texture));
        }
        if !self.negative_style.is_empty() {
            parts.push(format!("避免：{}", self.negative_style));
        }
        parts.join("；")
    }
}

// ─── Character Asset ───

/// 角色资产。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterAsset {
    pub id: String,
    pub name: String,
    /// 角色描述（性格、背景）。
    #[serde(default)]
    pub description: String,
    /// 性格关键词。
    #[serde(default)]
    pub personality: String,
    /// 外观描述（用于生成 Prompt）。
    #[serde(default)]
    pub visual_description: String,
    /// 参考图本地路径。
    #[serde(default)]
    pub reference_image_path: Option<String>,
    /// 角色专属风格覆盖（可选）。
    #[serde(default)]
    pub style_tokens: Option<StyleTokens>,
}

// ─── Scene Asset ───

/// 场景资产。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneAsset {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// 情绪氛围。
    #[serde(default)]
    pub mood: String,
    /// 时间段（"morning" / "noon" / "sunset" / "night"）。
    #[serde(default)]
    pub time_of_day: String,
    #[serde(default)]
    pub reference_image_path: Option<String>,
}

// ─── Reference Asset ───

/// 参考图资产。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceAsset {
    pub id: String,
    pub file_path: String,
    /// 来源（"user_upload" / "web_search" / "generated"）。
    #[serde(default)]
    pub source_type: String,
    /// Vision 分析结果（StyleTokens）。
    #[serde(default)]
    pub analysis: Option<StyleTokens>,
    /// 标签。
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: String,
}

// ─── Decision Record ───

/// 创作决策记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionRecord {
    pub id: String,
    /// 决策阶段（"requirement" / "visual" / "character" / "story"）。
    pub phase: String,
    /// 决策内容。
    pub decision: String,
    /// 决策理由。
    #[serde(default)]
    pub reason: String,
    /// 考虑过的替代方案。
    #[serde(default)]
    pub alternatives_considered: Vec<String>,
    pub decided_at: String,
}

// ─── Creative State Record ───

/// 创作状态记录（持久化实体）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreativeStateRecord {
    pub id: String,
    pub workspace_id: String,
    pub project_name: String,
    /// 项目类型（"poster" / "video" / "manga" / "brand" / "illustration"）。
    pub project_type: String,
    /// 目标受众。
    pub audience: String,
    /// 投放平台（"wechat" / "douyin" / "xiaohongshu" / "print" / "web"）。
    pub platform: String,
    /// 风格 Token。
    pub style_tokens: StyleTokens,
    /// 角色列表。
    pub characters: Vec<CharacterAsset>,
    /// 场景列表。
    pub scenes: Vec<SceneAsset>,
    /// 参考图列表。
    pub references: Vec<ReferenceAsset>,
    /// 约束条件。
    pub constraints: Vec<String>,
    /// 历史决策。
    pub history_decisions: Vec<DecisionRecord>,
    /// 乐观锁版本号。
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl CreativeStateRecord {
    /// 生成用于 Agent Prompt 注入的上下文摘要。
    pub fn to_prompt_context(&self) -> String {
        let mut ctx = String::from("[创作状态]\n");

        if !self.project_name.is_empty() {
            ctx.push_str(&format!("项目：{}\n", self.project_name));
        }
        if !self.project_type.is_empty() {
            ctx.push_str(&format!("类型：{}\n", self.project_type));
        }
        if !self.audience.is_empty() {
            ctx.push_str(&format!("受众：{}\n", self.audience));
        }
        if !self.platform.is_empty() {
            ctx.push_str(&format!("平台：{}\n", self.platform));
        }

        let style_frag = self.style_tokens.to_prompt_fragment();
        if !style_frag.is_empty() {
            ctx.push_str(&format!("视觉风格：{}\n", style_frag));
        }

        if !self.characters.is_empty() {
            ctx.push_str("角色：\n");
            for c in &self.characters {
                ctx.push_str(&format!("  - {}：{}\n", c.name, c.visual_description));
            }
        }

        if !self.scenes.is_empty() {
            ctx.push_str("场景：\n");
            for s in &self.scenes {
                ctx.push_str(&format!("  - {}：{}\n", s.name, s.description));
            }
        }

        if !self.constraints.is_empty() {
            ctx.push_str(&format!("约束：{}\n", self.constraints.join("；")));
        }

        if !self.history_decisions.is_empty() {
            ctx.push_str("近期决策：\n");
            for d in self.history_decisions.iter().rev().take(5) {
                ctx.push_str(&format!("  - [{}] {}\n", d.phase, d.decision));
            }
        }

        ctx
    }
}

// ─── Draft（创建/更新输入） ───

/// 创建创作状态的草稿。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreativeStateDraft {
    pub workspace_id: String,
    #[serde(default)]
    pub project_name: String,
    #[serde(default)]
    pub project_type: String,
    #[serde(default)]
    pub audience: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub style_tokens: StyleTokens,
    #[serde(default)]
    pub constraints: Vec<String>,
}

const NAME_MAX_LENGTH: usize = 200;
const FIELD_MAX_LENGTH: usize = 500;

impl CreativeStateDraft {
    pub fn validate(&self) -> Result<(), CreativeStateValidationError> {
        if self.workspace_id.is_empty() {
            return Err(CreativeStateValidationError::Required {
                field: "workspaceId".into(),
            });
        }
        if self.project_name.len() > NAME_MAX_LENGTH {
            return Err(CreativeStateValidationError::TooLong {
                field: "projectName".into(),
                max: NAME_MAX_LENGTH,
            });
        }
        if self.project_type.len() > FIELD_MAX_LENGTH {
            return Err(CreativeStateValidationError::TooLong {
                field: "projectType".into(),
                max: FIELD_MAX_LENGTH,
            });
        }
        Ok(())
    }
}
