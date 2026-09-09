use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::{
    application::{error::AppError, manga_service::MangaService},
    domain::manga::{
        CharacterProfileRecord, MangaProjectRecord, SceneRecord, ShotRecord, StoryBibleRecord,
    },
    ipc::error::IpcError,
};

// ── Helper ──

async fn with_manga<T, F>(app: AppHandle, operation: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce(&MangaService) -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let service = app.state::<MangaService>();
        operation(service.inner())
    })
    .await
    .map_err(|e| {
        eprintln!("manga native task failed: {e}");
        IpcError::task_failed()
    })?
    .map_err(IpcError::from)
}

// ── Project ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateMangaProjectRequest {
    pub workspace_id: String,
    pub title: String,
    pub classroom_id: Option<String>,
    pub theme: Option<String>,
    pub teaching_goal: Option<String>,
}

#[tauri::command]
pub async fn manga_v1_create_project(
    app: AppHandle,
    request: CreateMangaProjectRequest,
) -> Result<MangaProjectRecord, IpcError> {
    let ws = request.workspace_id.clone();
    let t = request.title.clone();
    let cid = request.classroom_id.clone();
    let th = request.theme.clone();
    let tg = request.teaching_goal.clone();
    with_manga(app, move |s| s.create_project(ws, t, cid, th, tg)).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListMangaProjectsRequest {
    pub workspace_id: String,
}

#[tauri::command]
pub async fn manga_v1_list_projects(
    app: AppHandle,
    request: ListMangaProjectsRequest,
) -> Result<Vec<MangaProjectRecord>, IpcError> {
    let ws = request.workspace_id.clone();
    with_manga(app, move |s| s.list_projects(&ws)).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetMangaProjectRequest {
    pub id: String,
}

#[tauri::command]
pub async fn manga_v1_get_project(
    app: AppHandle,
    request: GetMangaProjectRequest,
) -> Result<Option<MangaProjectRecord>, IpcError> {
    let id = request.id.clone();
    with_manga(app, move |s| s.get_project(&id)).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteMangaProjectRequest {
    pub id: String,
}

#[tauri::command]
pub async fn manga_v1_delete_project(
    app: AppHandle,
    request: DeleteMangaProjectRequest,
) -> Result<(), IpcError> {
    let id = request.id.clone();
    with_manga(app, move |s| s.delete_project(&id)).await
}

// ── Scene ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateSceneRequest {
    pub project_id: String,
    pub index: i32,
    pub title: String,
    pub summary: Option<String>,
    pub location: Option<String>,
}

#[tauri::command]
pub async fn manga_v1_create_scene(
    app: AppHandle,
    request: CreateSceneRequest,
) -> Result<SceneRecord, IpcError> {
    let pid = request.project_id.clone();
    let t = request.title.clone();
    let sm = request.summary.clone();
    let loc = request.location.clone();
    let idx = request.index;
    with_manga(app, move |s| s.create_scene(pid, idx, t, sm, loc)).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListScenesRequest {
    pub project_id: String,
}

#[tauri::command]
pub async fn manga_v1_list_scenes(
    app: AppHandle,
    request: ListScenesRequest,
) -> Result<Vec<SceneRecord>, IpcError> {
    let pid = request.project_id.clone();
    with_manga(app, move |s| s.list_scenes(&pid)).await
}

// ── Shot ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateShotRequest {
    pub scene_id: String,
    pub index: i32,
    pub shot_type: Option<String>,
    pub camera_motion: Option<String>,
    pub duration: Option<String>,
    pub prompt: Option<String>,
    pub negative_prompt: Option<String>,
}

#[tauri::command]
pub async fn manga_v1_create_shot(
    app: AppHandle,
    request: CreateShotRequest,
) -> Result<ShotRecord, IpcError> {
    let sid = request.scene_id.clone();
    let st = request.shot_type.clone();
    let cm = request.camera_motion.clone();
    let dur = request.duration.clone();
    let pr = request.prompt.clone();
    let np = request.negative_prompt.clone();
    let idx = request.index;
    with_manga(app, move |s| s.create_shot(sid, idx, st, cm, dur, pr, np)).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListShotsRequest {
    pub scene_id: String,
}

#[tauri::command]
pub async fn manga_v1_list_shots(
    app: AppHandle,
    request: ListShotsRequest,
) -> Result<Vec<ShotRecord>, IpcError> {
    let sid = request.scene_id.clone();
    with_manga(app, move |s| s.list_shots(&sid)).await
}

// ── Character ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateCharacterRequest {
    pub project_id: String,
    pub name: String,
    pub role: Option<String>,
    pub appearance: Option<String>,
    pub personality: Option<String>,
    pub consistency_prompt: Option<String>,
}

#[tauri::command]
pub async fn manga_v1_create_character(
    app: AppHandle,
    request: CreateCharacterRequest,
) -> Result<CharacterProfileRecord, IpcError> {
    let pid = request.project_id.clone();
    let nm = request.name.clone();
    let rl = request.role.clone();
    let ap = request.appearance.clone();
    let pr = request.personality.clone();
    let cp = request.consistency_prompt.clone();
    with_manga(app, move |s| s.create_character(pid, nm, rl, ap, pr, cp)).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListCharactersRequest {
    pub project_id: String,
}

#[tauri::command]
pub async fn manga_v1_list_characters(
    app: AppHandle,
    request: ListCharactersRequest,
) -> Result<Vec<CharacterProfileRecord>, IpcError> {
    let pid = request.project_id.clone();
    with_manga(app, move |s| s.list_characters(&pid)).await
}

// ── StoryBible ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertStoryBibleRequest {
    pub project_id: String,
    pub logline: Option<String>,
    pub synopsis: Option<String>,
    pub style_guide: Option<String>,
    pub visual_style: Option<String>,
    pub tone: Option<String>,
}

#[tauri::command]
pub async fn manga_v1_upsert_story_bible(
    app: AppHandle,
    request: UpsertStoryBibleRequest,
) -> Result<StoryBibleRecord, IpcError> {
    let pid = request.project_id.clone();
    let ll = request.logline.clone();
    let sy = request.synopsis.clone();
    let sg = request.style_guide.clone();
    let vs = request.visual_style.clone();
    let tn = request.tone.clone();
    with_manga(app, move |s| s.upsert_story_bible(pid, ll, sy, sg, vs, tn)).await
}
