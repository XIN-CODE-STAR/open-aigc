#![allow(dead_code)]
use std::path::Path;
use std::sync::Mutex;

use crate::adapters::sqlite::SqliteMangaRepository;
use crate::application::error::AppError;
use crate::domain::manga::{
    CharacterProfileDraft, CharacterProfileRecord, MangaProjectDraft, MangaProjectRecord,
    SceneDraft, SceneRecord, ShotAssetBindingDraft, ShotAssetBindingRecord, ShotDraft, ShotRecord,
    ShotStatus, StoryBibleDraft, StoryBibleRecord,
};
use crate::ports::manga_repository::MangaRepository;

/// 项目统计信息。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStats {
    pub scene_count: usize,
    pub character_count: usize,
    pub total_shots: usize,
    pub completed_shots: usize,
    pub failed_shots: usize,
}

pub struct MangaService {
    repository: Mutex<SqliteMangaRepository>,
}

impl MangaService {
    pub fn new(repository: SqliteMangaRepository) -> Self {
        Self {
            repository: Mutex::new(repository),
        }
    }

    pub fn reload(&self, database_path: &Path) -> Result<(), AppError> {
        let new_repo = SqliteMangaRepository::open(database_path)?;
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        *repo = new_repo;
        Ok(())
    }

    // ── Project ──

    pub fn create_project(
        &self,
        workspace_id: String,
        title: String,
        classroom_id: Option<String>,
        theme: Option<String>,
        teaching_goal: Option<String>,
    ) -> Result<MangaProjectRecord, AppError> {
        let draft =
            MangaProjectDraft::try_new(workspace_id, title, classroom_id, theme, teaching_goal)?;
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.create_project(draft).map_err(AppError::from)
    }

    pub fn list_projects(&self, workspace_id: &str) -> Result<Vec<MangaProjectRecord>, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.list_projects(workspace_id).map_err(AppError::from)
    }

    pub fn get_project(&self, id: &str) -> Result<Option<MangaProjectRecord>, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.get_project(id).map_err(AppError::from)
    }

    pub fn delete_project(&self, id: &str) -> Result<(), AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.delete_project(id).map_err(AppError::from)
    }

    /// 绑定项目到班级。
    pub fn bind_to_classroom(
        &self,
        project_id: &str,
        classroom_id: Option<&str>,
    ) -> Result<MangaProjectRecord, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        // 获取现有项目
        let project = repo.get_project(project_id)?.ok_or_else(|| {
            AppError::from(
                crate::ports::manga_repository::MangaRepositoryError::NotFound("project".into()),
            )
        })?;

        // 更新 classroom_id（需要在 domain 层添加此功能）
        // 暂时返回项目
        Ok(project)
    }

    /// 获取项目的统计信息。
    pub fn get_project_stats(&self, project_id: &str) -> Result<ProjectStats, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;

        let scenes = repo.list_scenes(project_id)?;
        let characters = repo.list_characters(project_id)?;

        let mut total_shots = 0;
        let mut completed_shots = 0;
        let mut failed_shots = 0;

        for scene in &scenes {
            let shots = repo.list_shots(&scene.id)?;
            total_shots += shots.len();
            completed_shots += shots
                .iter()
                .filter(|s| s.status == crate::domain::manga::ShotStatus::Completed)
                .count();
            failed_shots += shots
                .iter()
                .filter(|s| s.status == crate::domain::manga::ShotStatus::Failed)
                .count();
        }

        Ok(ProjectStats {
            scene_count: scenes.len(),
            character_count: characters.len(),
            total_shots,
            completed_shots,
            failed_shots,
        })
    }

    // ── StoryBible ──

    pub fn upsert_story_bible(
        &self,
        project_id: String,
        logline: Option<String>,
        synopsis: Option<String>,
        style_guide: Option<String>,
        visual_style: Option<String>,
        tone: Option<String>,
    ) -> Result<StoryBibleRecord, AppError> {
        let draft = StoryBibleDraft::try_new(
            project_id,
            logline,
            synopsis,
            style_guide,
            visual_style,
            tone,
        )?;
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.upsert_story_bible(draft).map_err(AppError::from)
    }

    pub fn get_story_bible(&self, project_id: &str) -> Result<Option<StoryBibleRecord>, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.get_story_bible(project_id).map_err(AppError::from)
    }

    // ── Character ──

    pub fn create_character(
        &self,
        project_id: String,
        name: String,
        role: Option<String>,
        appearance: Option<String>,
        personality: Option<String>,
        consistency_prompt: Option<String>,
    ) -> Result<CharacterProfileRecord, AppError> {
        let draft = CharacterProfileDraft::try_new(
            project_id,
            name,
            role,
            appearance,
            personality,
            consistency_prompt,
        )?;
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.create_character(draft).map_err(AppError::from)
    }

    pub fn list_characters(
        &self,
        project_id: &str,
    ) -> Result<Vec<CharacterProfileRecord>, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.list_characters(project_id).map_err(AppError::from)
    }

    // ── Scene ──

    pub fn create_scene(
        &self,
        project_id: String,
        index: i32,
        title: String,
        summary: Option<String>,
        location: Option<String>,
    ) -> Result<SceneRecord, AppError> {
        let draft = SceneDraft::try_new(project_id, index, title, summary, location)?;
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.create_scene(draft).map_err(AppError::from)
    }

    pub fn list_scenes(&self, project_id: &str) -> Result<Vec<SceneRecord>, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.list_scenes(project_id).map_err(AppError::from)
    }

    // ── Shot ──

    pub fn create_shot(
        &self,
        scene_id: String,
        index: i32,
        shot_type: Option<String>,
        camera_motion: Option<String>,
        duration: Option<String>,
        prompt: Option<String>,
        negative_prompt: Option<String>,
    ) -> Result<ShotRecord, AppError> {
        let draft = ShotDraft::try_new(
            scene_id,
            index,
            shot_type,
            camera_motion,
            duration,
            prompt,
            negative_prompt,
        )?;
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.create_shot(draft).map_err(AppError::from)
    }

    pub fn list_shots(&self, scene_id: &str) -> Result<Vec<ShotRecord>, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.list_shots(scene_id).map_err(AppError::from)
    }

    pub fn update_shot_prompt(
        &self,
        id: &str,
        prompt: &str,
        negative_prompt: Option<&str>,
    ) -> Result<ShotRecord, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.update_shot_prompt(id, prompt, negative_prompt)
            .map_err(AppError::from)
    }

    pub fn update_shot_status(&self, id: &str, status: ShotStatus) -> Result<ShotRecord, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.update_shot_status(id, status).map_err(AppError::from)
    }

    // ── Binding ──

    pub fn bind_asset(
        &self,
        shot_id: String,
        asset_id: String,
        kind: String,
    ) -> Result<ShotAssetBindingRecord, AppError> {
        let draft = ShotAssetBindingDraft::try_new(shot_id, asset_id, kind)?;
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.bind_asset(draft).map_err(AppError::from)
    }

    pub fn list_bindings(&self, shot_id: &str) -> Result<Vec<ShotAssetBindingRecord>, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.list_bindings(shot_id).map_err(AppError::from)
    }
}
