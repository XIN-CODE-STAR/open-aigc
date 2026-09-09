use crate::domain::manga::{
    CharacterProfileDraft, CharacterProfileRecord, MangaError, MangaProjectDraft,
    MangaProjectRecord, MangaProjectStatus, SceneDraft, SceneRecord, ShotAssetBindingDraft,
    ShotAssetBindingRecord, ShotDraft, ShotRecord, ShotStatus, StoryBibleDraft, StoryBibleRecord,
};
use crate::ports::persistence::PersistenceError;

#[derive(Debug, thiserror::Error)]
pub enum MangaRepositoryError {
    #[error("manga entity {0} not found")]
    NotFound(String),
    #[error(transparent)]
    Validation(#[from] MangaError),
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

/// AI 漫剧项目仓储端口。
pub trait MangaRepository: Send {
    // ── Project ──
    fn create_project(
        &mut self,
        draft: MangaProjectDraft,
    ) -> Result<MangaProjectRecord, MangaRepositoryError>;
    fn get_project(&mut self, id: &str)
        -> Result<Option<MangaProjectRecord>, MangaRepositoryError>;
    fn list_projects(
        &mut self,
        workspace_id: &str,
    ) -> Result<Vec<MangaProjectRecord>, MangaRepositoryError>;
    fn update_project_status(
        &mut self,
        id: &str,
        status: MangaProjectStatus,
    ) -> Result<MangaProjectRecord, MangaRepositoryError>;
    fn delete_project(&mut self, id: &str) -> Result<(), MangaRepositoryError>;

    // ── StoryBible ──
    fn upsert_story_bible(
        &mut self,
        draft: StoryBibleDraft,
    ) -> Result<StoryBibleRecord, MangaRepositoryError>;
    fn get_story_bible(
        &mut self,
        project_id: &str,
    ) -> Result<Option<StoryBibleRecord>, MangaRepositoryError>;

    // ── CharacterProfile ──
    fn create_character(
        &mut self,
        draft: CharacterProfileDraft,
    ) -> Result<CharacterProfileRecord, MangaRepositoryError>;
    fn list_characters(
        &mut self,
        project_id: &str,
    ) -> Result<Vec<CharacterProfileRecord>, MangaRepositoryError>;
    fn delete_character(&mut self, id: &str) -> Result<(), MangaRepositoryError>;

    // ── Scene ──
    fn create_scene(&mut self, draft: SceneDraft) -> Result<SceneRecord, MangaRepositoryError>;
    fn list_scenes(&mut self, project_id: &str) -> Result<Vec<SceneRecord>, MangaRepositoryError>;
    fn delete_scene(&mut self, id: &str) -> Result<(), MangaRepositoryError>;

    // ── Shot ──
    fn create_shot(&mut self, draft: ShotDraft) -> Result<ShotRecord, MangaRepositoryError>;
    fn list_shots(&mut self, scene_id: &str) -> Result<Vec<ShotRecord>, MangaRepositoryError>;
    fn update_shot_status(
        &mut self,
        id: &str,
        status: ShotStatus,
    ) -> Result<ShotRecord, MangaRepositoryError>;
    fn update_shot_prompt(
        &mut self,
        id: &str,
        prompt: &str,
        negative_prompt: Option<&str>,
    ) -> Result<ShotRecord, MangaRepositoryError>;
    fn delete_shot(&mut self, id: &str) -> Result<(), MangaRepositoryError>;

    // ── ShotAssetBinding ──
    fn bind_asset(
        &mut self,
        draft: ShotAssetBindingDraft,
    ) -> Result<ShotAssetBindingRecord, MangaRepositoryError>;
    fn list_bindings(
        &mut self,
        shot_id: &str,
    ) -> Result<Vec<ShotAssetBindingRecord>, MangaRepositoryError>;
    fn unbind_asset(&mut self, id: &str) -> Result<(), MangaRepositoryError>;
}
