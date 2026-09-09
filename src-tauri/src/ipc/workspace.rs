use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::{
    application::{error::AppError, workspace_service::WorkspaceService},
    domain::workspace::{WorkspaceProfile, WorkspaceStatus},
    ipc::error::IpcError,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRootPathResponse {
    pub workspace_dir: String,
    pub managed_files_dir: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InitializeWorkspaceRequest {
    pub workspace_name: String,
    pub teacher_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenameWorkspaceRequest {
    pub workspace_name: String,
}

#[tauri::command]
pub async fn workspace_v1_get_status(app: AppHandle) -> Result<WorkspaceStatus, IpcError> {
    with_workspace_service(app, WorkspaceService::get_status).await
}

#[tauri::command]
pub async fn workspace_v1_initialize(
    app: AppHandle,
    request: InitializeWorkspaceRequest,
) -> Result<WorkspaceProfile, IpcError> {
    with_workspace_service(app, move |service| {
        service.initialize(request.workspace_name, request.teacher_name)
    })
    .await
}

/// 重命名当前工作空间。教师自助修改。
/// 名称校验复用 `NewWorkspace::try_new`（非空、长度、控制字符）。
/// 未初始化时由后端 `PersistenceError::NotFound` 转 `IpcError`。
#[tauri::command]
pub async fn workspace_v1_rename(
    app: AppHandle,
    request: RenameWorkspaceRequest,
) -> Result<WorkspaceProfile, IpcError> {
    with_workspace_service(app, move |service| {
        service.rename_workspace(request.workspace_name)
    })
    .await
}

#[tauri::command]
pub async fn workspace_v1_get_root_path(
    app: AppHandle,
) -> Result<WorkspaceRootPathResponse, IpcError> {
    let workspace_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| IpcError::from(AppError::new("get app data dir", e)))?
        .join("workspace");
    let managed_files_dir = workspace_dir.join("managed-files");
    Ok(WorkspaceRootPathResponse {
        workspace_dir: workspace_dir.to_string_lossy().to_string(),
        managed_files_dir: managed_files_dir.to_string_lossy().to_string(),
    })
}

async fn with_workspace_service<T, F>(app: AppHandle, operation: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce(&WorkspaceService) -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let service = app.state::<WorkspaceService>();
        operation(service.inner())
    })
    .await
    .map_err(|error| {
        eprintln!("workspace native task failed: {error}");
        IpcError::task_failed()
    })?
    .map_err(IpcError::from)
}
