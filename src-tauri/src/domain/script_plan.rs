//! ScriptPlan 领域模型 — 脚本解析结果的中间表示。
#![allow(dead_code)]
//!
//! ScriptPlan 是 parse_script 的输出，也是 ApplyScriptPlan 的输入。
//! 它是"脚本解析"和"项目持久化"之间的桥梁。
//!
//! 数据流：
//! Script Text → ScriptParser → ScriptPlan → user confirm → ApplyScriptPlan → MangaProject + CanvasNodes

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ──────────────────────────────────────────────────────────────────
// Errors
// ──────────────────────────────────────────────────────────────────

/// ScriptPlan 验证错误。
#[derive(Debug, Error)]
pub enum ScriptPlanError {
    #[error("script text is empty")]
    EmptyScript,

    #[error("no scenes could be extracted from the script")]
    NoScenes,

    #[error("scene {scene_index} has no shots")]
    EmptyShots { scene_index: u32 },

    #[error("too many scenes: {count} (max {max})")]
    TooManyScenes { count: usize, max: usize },

    #[error("scene {scene_index} has too many shots: {count} (max {max})")]
    TooManyShots {
        scene_index: u32,
        count: usize,
        max: usize,
    },

    #[error("title exceeds {max} characters")]
    TitleTooLong { max: usize },

    #[error("scene title exceeds {max} characters")]
    SceneTitleTooLong { max: usize },

    #[error("shot description exceeds {max} characters")]
    ShotDescriptionTooLong { max: usize },

    #[error("visual_intent exceeds {max} characters")]
    VisualIntentTooLong { max: usize },
}

// ──────────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────────

const TITLE_MAX: usize = 200;
const SCENE_TITLE_MAX: usize = 200;
const SHOT_DESC_MAX: usize = 8000;
const PROMPT_MAX: usize = 8000;
pub const MAX_SCENES: usize = 50;
pub const MAX_SHOTS_PER_SCENE: usize = 20;

// ──────────────────────────────────────────────────────────────────
// ScriptPlan — 脚本解析的结构化结果
// ──────────────────────────────────────────────────────────────────

/// 脚本解析计划 — 从文本剧本解析出的结构化中间表示。
///
/// 不直接持久化，是 ScriptParser → ApplyScriptPlan 之间的传输对象。
/// 序列化为 JSON 后通过 Agent 工具返回给 LLM，用户确认后再调用 ApplyScriptPlan。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptPlan {
    /// 唯一标识（用于追踪和日志）。
    pub id: String,
    /// 关联的会话 ID。
    pub conversation_id: Option<String>,
    /// 建议的项目标题。
    pub title: Option<String>,
    /// 全局风格提示。
    pub style: Option<String>,
    /// 解析出的场景列表。
    pub scenes: Vec<PlannedScene>,
    /// 总镜头数（方便前端展示）。
    pub total_shots: usize,
    /// 解析时间。
    pub created_at: String,
}

// ──────────────────────────────────────────────────────────────────
// PlannedScene — 计划中的场景
// ──────────────────────────────────────────────────────────────────

/// 计划中的场景 — 对应 MangaProject 的 Scene。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedScene {
    /// 场景序号（1-based）。
    pub index: u32,
    /// 场景标题。
    pub title: String,
    /// 场景摘要。
    pub summary: Option<String>,
    /// 场景位置（内景/外景/室内/室外 等）。
    pub location: Option<String>,
    /// 场景内的镜头列表。
    pub shots: Vec<PlannedShot>,
}

// ──────────────────────────────────────────────────────────────────
// PlannedShot — 计划中的镜头
// ──────────────────────────────────────────────────────────────────

/// 计划中的镜头 — 对应 MangaProject 的 Shot。
///
/// 保存"我要拍什么"（语义意图），PromptCompiler 决定"具体怎么告诉模型拍"。
/// visual_intent 不是最终 prompt — PromptCompiler 负责将它编译为模型可理解的 prompt。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedShot {
    /// 镜头序号（1-based，场景内）。
    pub index: u32,
    /// 镜头描述（原始文本，中文）。
    pub description: String,
    /// 镜头类型："wide", "close-up", "medium", "extreme-close-up", "aerial" 等。
    pub shot_type: Option<String>,
    /// 运镜方式："static", "dolly-in", "tracking", "orbit", "pan", "tilt", "crane" 等。
    pub camera_motion: Option<String>,
    /// 视觉意图 — 表达"我要拍什么"，不是最终 prompt。
    /// PromptCompiler 会将它编译为最终 prompt（结合风格、角色、场景等上下文）。
    pub visual_intent: Option<String>,
    /// 建议时长（秒）。
    pub duration: Option<f32>,
    /// 引用的角色名称/ID 列表。
    pub character_refs: Vec<String>,
    /// 镜头级风格覆盖（优先于全局 style）。
    pub style_override: Option<String>,
}

// ──────────────────────────────────────────────────────────────────
// ScriptPlanDraft — 用于构造 ScriptPlan（内部）
// ──────────────────────────────────────────────────────────────────

/// 构造 ScriptPlan 的输入（ScriptParserService 内部使用）。
#[derive(Debug, Clone)]
pub struct ScriptPlanDraft {
    pub conversation_id: Option<String>,
    pub title: Option<String>,
    pub style: Option<String>,
    pub scenes: Vec<PlannedScene>,
}

impl ScriptPlanDraft {
    /// 验证并转换为 ScriptPlan。
    pub fn try_into_plan(self) -> Result<ScriptPlan, ScriptPlanError> {
        if self.scenes.is_empty() {
            return Err(ScriptPlanError::NoScenes);
        }

        if self.scenes.len() > MAX_SCENES {
            return Err(ScriptPlanError::TooManyScenes {
                count: self.scenes.len(),
                max: MAX_SCENES,
            });
        }

        if let Some(ref t) = self.title {
            if t.len() > TITLE_MAX {
                return Err(ScriptPlanError::TitleTooLong { max: TITLE_MAX });
            }
        }

        let mut total_shots = 0usize;
        for scene in &self.scenes {
            if scene.title.len() > SCENE_TITLE_MAX {
                return Err(ScriptPlanError::SceneTitleTooLong {
                    max: SCENE_TITLE_MAX,
                });
            }

            if scene.shots.is_empty() {
                return Err(ScriptPlanError::EmptyShots {
                    scene_index: scene.index,
                });
            }

            if scene.shots.len() > MAX_SHOTS_PER_SCENE {
                return Err(ScriptPlanError::TooManyShots {
                    scene_index: scene.index,
                    count: scene.shots.len(),
                    max: MAX_SHOTS_PER_SCENE,
                });
            }

            for shot in &scene.shots {
                if shot.description.len() > SHOT_DESC_MAX {
                    return Err(ScriptPlanError::ShotDescriptionTooLong { max: SHOT_DESC_MAX });
                }
                if let Some(ref v) = shot.visual_intent {
                    if v.len() > PROMPT_MAX {
                        return Err(ScriptPlanError::VisualIntentTooLong { max: PROMPT_MAX });
                    }
                }
            }

            total_shots += scene.shots.len();
        }

        Ok(ScriptPlan {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: self.conversation_id,
            title: self.title,
            style: self.style,
            scenes: self.scenes,
            total_shots,
            created_at: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        })
    }
}

// ──────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_scene(index: u32, shots: Vec<PlannedShot>) -> PlannedScene {
        PlannedScene {
            index,
            title: format!("Scene {index}"),
            summary: None,
            location: None,
            shots,
        }
    }

    fn make_shot(index: u32) -> PlannedShot {
        PlannedShot {
            index,
            description: "A character stands in the rain.".to_owned(),
            shot_type: Some("medium".to_owned()),
            camera_motion: Some("static".to_owned()),
            visual_intent: Some(
                "a character standing in heavy rain, cinematic lighting".to_owned(),
            ),
            duration: Some(3.0),
            character_refs: vec![],
            style_override: None,
        }
    }

    #[test]
    fn valid_plan() {
        let draft = ScriptPlanDraft {
            conversation_id: None,
            title: Some("Test".to_owned()),
            style: Some("anime".to_owned()),
            scenes: vec![make_scene(1, vec![make_shot(1), make_shot(2)])],
        };
        let plan = draft.try_into_plan().unwrap();
        assert_eq!(plan.total_shots, 2);
        assert_eq!(plan.scenes.len(), 1);
    }

    #[test]
    fn empty_scenes() {
        let draft = ScriptPlanDraft {
            conversation_id: None,
            title: None,
            style: None,
            scenes: vec![],
        };
        assert!(matches!(
            draft.try_into_plan(),
            Err(ScriptPlanError::NoScenes)
        ));
    }

    #[test]
    fn empty_shots() {
        let draft = ScriptPlanDraft {
            conversation_id: None,
            title: None,
            style: None,
            scenes: vec![make_scene(1, vec![])],
        };
        assert!(matches!(
            draft.try_into_plan(),
            Err(ScriptPlanError::EmptyShots { scene_index: 1 })
        ));
    }

    #[test]
    fn too_many_scenes() {
        let scenes: Vec<_> = (1..=51)
            .map(|i| make_scene(i, vec![make_shot(1)]))
            .collect();
        let draft = ScriptPlanDraft {
            conversation_id: None,
            title: None,
            style: None,
            scenes,
        };
        assert!(matches!(
            draft.try_into_plan(),
            Err(ScriptPlanError::TooManyScenes { count: 51, max: 50 })
        ));
    }

    #[test]
    fn title_too_long() {
        let draft = ScriptPlanDraft {
            conversation_id: None,
            title: Some("x".repeat(TITLE_MAX + 1)),
            style: None,
            scenes: vec![make_scene(1, vec![make_shot(1)])],
        };
        assert!(matches!(
            draft.try_into_plan(),
            Err(ScriptPlanError::TitleTooLong { .. })
        ));
    }

    #[test]
    fn serialization_roundtrip() {
        let draft = ScriptPlanDraft {
            conversation_id: Some("conv-1".to_owned()),
            title: Some("My Script".to_owned()),
            style: Some("cyberpunk".to_owned()),
            scenes: vec![make_scene(
                1,
                vec![PlannedShot {
                    index: 1,
                    description: "Opening shot".to_owned(),
                    shot_type: Some("wide".to_owned()),
                    camera_motion: Some("dolly-in".to_owned()),
                    visual_intent: Some("cyberpunk city at night".to_owned()),
                    duration: Some(5.0),
                    character_refs: vec!["hero".to_owned()],
                    style_override: None,
                }],
            )],
        };
        let plan = draft.try_into_plan().unwrap();
        let json = serde_json::to_string(&plan).unwrap();
        let back: ScriptPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(back.title, plan.title);
        assert_eq!(back.total_shots, 1);
        assert_eq!(back.scenes[0].shots[0].character_refs, vec!["hero"]);
        assert_eq!(
            back.scenes[0].shots[0].visual_intent.as_deref(),
            Some("cyberpunk city at night")
        );
    }
}
