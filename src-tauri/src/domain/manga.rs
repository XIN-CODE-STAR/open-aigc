//! AI 漫剧项目领域模型。
#![allow(dead_code)]
//!
//! 实体层级：MangaProject → StoryBible / CharacterProfile / Scene → Shot → ShotAssetBinding

use serde::{Deserialize, Serialize};
use thiserror::Error;

const TITLE_MAX: usize = 200;
const TEXT_MAX: usize = 8000;
const NAME_MAX: usize = 120;

// ──────────────────────────────────────────────────────────────────
// MangaProject
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MangaProjectStatus {
    Draft,
    InProgress,
    Completed,
    Archived,
}

impl MangaProjectStatus {
    pub fn parse(v: &str) -> Result<Self, MangaError> {
        match v {
            "draft" => Ok(Self::Draft),
            "in-progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "archived" => Ok(Self::Archived),
            _ => Err(MangaError::InvalidChoice("status")),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::InProgress => "in-progress",
            Self::Completed => "completed",
            Self::Archived => "archived",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MangaProjectDraft {
    pub workspace_id: String,
    pub classroom_id: Option<String>,
    pub title: String,
    pub theme: Option<String>,
    pub teaching_goal: Option<String>,
}

impl MangaProjectDraft {
    pub fn try_new(
        workspace_id: String,
        title: String,
        classroom_id: Option<String>,
        theme: Option<String>,
        teaching_goal: Option<String>,
    ) -> Result<Self, MangaError> {
        let workspace_id = require_uuid(&workspace_id, "workspaceId")?;
        let title = require_text(&title, "title", TITLE_MAX)?;
        Ok(Self {
            workspace_id,
            title,
            classroom_id: classroom_id
                .map(|c| require_uuid(&c, "classroomId"))
                .transpose()?,
            theme: optional_text(theme, "theme", TEXT_MAX),
            teaching_goal: optional_text(teaching_goal, "teachingGoal", TEXT_MAX),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MangaProjectRecord {
    pub id: String,
    pub workspace_id: String,
    pub classroom_id: Option<String>,
    pub title: String,
    pub theme: Option<String>,
    pub teaching_goal: Option<String>,
    pub status: MangaProjectStatus,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

// ──────────────────────────────────────────────────────────────────
// StoryBible
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryBibleDraft {
    pub project_id: String,
    pub logline: Option<String>,
    pub synopsis: Option<String>,
    pub style_guide: Option<String>,
    pub visual_style: Option<String>,
    pub tone: Option<String>,
}

impl StoryBibleDraft {
    pub fn try_new(
        project_id: String,
        logline: Option<String>,
        synopsis: Option<String>,
        style_guide: Option<String>,
        visual_style: Option<String>,
        tone: Option<String>,
    ) -> Result<Self, MangaError> {
        Ok(Self {
            project_id: require_uuid(&project_id, "projectId")?,
            logline: optional_text(logline, "logline", TEXT_MAX),
            synopsis: optional_text(synopsis, "synopsis", TEXT_MAX),
            style_guide: optional_text(style_guide, "styleGuide", TEXT_MAX),
            visual_style: optional_text(visual_style, "visualStyle", TEXT_MAX),
            tone: optional_text(tone, "tone", 500),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryBibleRecord {
    pub id: String,
    pub project_id: String,
    pub logline: Option<String>,
    pub synopsis: Option<String>,
    pub style_guide: Option<String>,
    pub visual_style: Option<String>,
    pub tone: Option<String>,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

// ──────────────────────────────────────────────────────────────────
// CharacterProfile
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterProfileDraft {
    pub project_id: String,
    pub name: String,
    pub role: Option<String>,
    pub appearance: Option<String>,
    pub personality: Option<String>,
    pub consistency_prompt: Option<String>,
}

impl CharacterProfileDraft {
    pub fn try_new(
        project_id: String,
        name: String,
        role: Option<String>,
        appearance: Option<String>,
        personality: Option<String>,
        consistency_prompt: Option<String>,
    ) -> Result<Self, MangaError> {
        Ok(Self {
            project_id: require_uuid(&project_id, "projectId")?,
            name: require_text(&name, "name", NAME_MAX)?,
            role: optional_text(role, "role", 200),
            appearance: optional_text(appearance, "appearance", TEXT_MAX),
            personality: optional_text(personality, "personality", TEXT_MAX),
            consistency_prompt: optional_text(consistency_prompt, "consistencyPrompt", TEXT_MAX),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterProfileRecord {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub role: Option<String>,
    pub appearance: Option<String>,
    pub personality: Option<String>,
    pub consistency_prompt: Option<String>,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

// ──────────────────────────────────────────────────────────────────
// Scene
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneDraft {
    pub project_id: String,
    pub index: i32,
    pub title: String,
    pub summary: Option<String>,
    pub location: Option<String>,
}

impl SceneDraft {
    pub fn try_new(
        project_id: String,
        index: i32,
        title: String,
        summary: Option<String>,
        location: Option<String>,
    ) -> Result<Self, MangaError> {
        if index < 0 {
            return Err(MangaError::InvalidChoice("index"));
        }
        Ok(Self {
            project_id: require_uuid(&project_id, "projectId")?,
            index,
            title: require_text(&title, "title", TITLE_MAX)?,
            summary: optional_text(summary, "summary", TEXT_MAX),
            location: optional_text(location, "location", 500),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneRecord {
    pub id: String,
    pub project_id: String,
    pub index: i32,
    pub title: String,
    pub summary: Option<String>,
    pub location: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ──────────────────────────────────────────────────────────────────
// Shot
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShotStatus {
    Draft,
    Ready,
    Generating,
    Completed,
    Failed,
}

impl ShotStatus {
    pub fn parse(v: &str) -> Result<Self, MangaError> {
        match v {
            "draft" => Ok(Self::Draft),
            "ready" => Ok(Self::Ready),
            "generating" => Ok(Self::Generating),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            _ => Err(MangaError::InvalidChoice("shotStatus")),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Ready => "ready",
            Self::Generating => "generating",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShotDraft {
    pub scene_id: String,
    pub index: i32,
    pub shot_type: Option<String>,
    pub camera_motion: Option<String>,
    pub duration: Option<String>,
    pub prompt: Option<String>,
    pub negative_prompt: Option<String>,
}

impl ShotDraft {
    pub fn try_new(
        scene_id: String,
        index: i32,
        shot_type: Option<String>,
        camera_motion: Option<String>,
        duration: Option<String>,
        prompt: Option<String>,
        negative_prompt: Option<String>,
    ) -> Result<Self, MangaError> {
        if index < 0 {
            return Err(MangaError::InvalidChoice("index"));
        }
        Ok(Self {
            scene_id: require_uuid(&scene_id, "sceneId")?,
            index,
            shot_type: optional_text(shot_type, "shotType", 100),
            camera_motion: optional_text(camera_motion, "cameraMotion", 200),
            duration: optional_text(duration, "duration", 50),
            prompt: optional_text(prompt, "prompt", TEXT_MAX),
            negative_prompt: optional_text(negative_prompt, "negativePrompt", TEXT_MAX),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShotRecord {
    pub id: String,
    pub scene_id: String,
    pub index: i32,
    pub shot_type: Option<String>,
    pub camera_motion: Option<String>,
    pub duration: Option<String>,
    pub prompt: Option<String>,
    pub negative_prompt: Option<String>,
    pub status: ShotStatus,
    pub created_at: String,
    pub updated_at: String,
}

// ──────────────────────────────────────────────────────────────────
// ShotAssetBinding
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShotAssetBindingDraft {
    pub shot_id: String,
    pub asset_id: String,
    pub kind: String,
}

impl ShotAssetBindingDraft {
    pub fn try_new(shot_id: String, asset_id: String, kind: String) -> Result<Self, MangaError> {
        Ok(Self {
            shot_id: require_uuid(&shot_id, "shotId")?,
            asset_id: require_uuid(&asset_id, "assetId")?,
            kind: require_text(&kind, "kind", 100)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShotAssetBindingRecord {
    pub id: String,
    pub shot_id: String,
    pub asset_id: String,
    pub kind: String,
    pub created_at: String,
}

// ──────────────────────────────────────────────────────────────────
// Error
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MangaError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} must be at most {max} characters")]
    TooLong { field: &'static str, max: usize },
    #[error("{0} is invalid")]
    InvalidChoice(&'static str),
    #[error("{0} is not a valid uuid")]
    InvalidUuid(&'static str),
}

impl MangaError {
    pub fn user_message(&self) -> String {
        match self {
            Self::Required { field } => format!("{}不能为空。", label(field)),
            Self::TooLong { field, max } => format!("{}不能超过 {max} 个字符。", label(field)),
            Self::InvalidChoice(f) => format!("{}无效。", label(f)),
            Self::InvalidUuid(f) => format!("{}格式无效。", label(f)),
        }
    }
}

fn label(f: &str) -> &'static str {
    match f {
        "workspaceId" => "工作空间标识",
        "classroomId" => "班级标识",
        "projectId" => "项目标识",
        "sceneId" => "场景标识",
        "shotId" => "镜头标识",
        "assetId" => "资产标识",
        "title" => "标题",
        "name" => "名称",
        "status" | "shotStatus" => "状态",
        "index" => "序号",
        "kind" => "类型",
        _ => "字段",
    }
}

fn require_uuid(v: &str, field: &'static str) -> Result<String, MangaError> {
    let t = v.trim();
    if t.is_empty() {
        return Err(MangaError::Required { field });
    }
    if t.len() != 36 || !t.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err(MangaError::InvalidUuid(field));
    }
    Ok(t.to_owned())
}

fn require_text(v: &str, field: &'static str, max: usize) -> Result<String, MangaError> {
    let t = v.trim();
    if t.is_empty() {
        return Err(MangaError::Required { field });
    }
    if t.chars().count() > max {
        return Err(MangaError::TooLong { field, max });
    }
    Ok(t.to_owned())
}

fn optional_text(v: Option<String>, _field: &str, _max: usize) -> Option<String> {
    v.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "123e4567-e89b-42d3-a456-426614174000";

    #[test]
    fn manga_project_draft_valid() {
        let d = MangaProjectDraft::try_new(
            UUID.to_owned(),
            "环保动画".to_owned(),
            None,
            Some("自然".to_owned()),
            None,
        )
        .unwrap();
        assert_eq!(d.title, "环保动画");
    }

    #[test]
    fn manga_project_draft_rejects_empty_title() {
        assert!(
            MangaProjectDraft::try_new(UUID.to_owned(), "".to_owned(), None, None, None).is_err()
        );
    }

    #[test]
    fn shot_draft_rejects_negative_index() {
        assert!(ShotDraft::try_new(UUID.to_owned(), -1, None, None, None, None, None).is_err());
    }

    #[test]
    fn character_draft_valid() {
        let d = CharacterProfileDraft::try_new(
            UUID.to_owned(),
            "小明".to_owned(),
            Some("主角".to_owned()),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(d.name, "小明");
    }

    #[test]
    fn status_roundtrip() {
        assert_eq!(
            MangaProjectStatus::parse("draft").unwrap(),
            MangaProjectStatus::Draft
        );
        assert_eq!(
            ShotStatus::parse("generating").unwrap(),
            ShotStatus::Generating
        );
    }
}
