use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::{
    application::{
        backup_service::BackupService, error::AppError, service_reloader::ServiceReloader,
        workspace_service::WorkspaceService,
    },
    domain::backup::{BackupSummary, RestorePreview, RestoreSummary},
    ipc::error::IpcError,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateBackupRequest {
    /// 用户通过文件保存对话框选择的归档绝对路径。
    pub archive_path: String,
    /// 用户输入的备注（可选）。
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewRestoreRequest {
    pub archive_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreBackupRequest {
    pub archive_path: String,
}

#[tauri::command]
pub async fn backup_v1_create(
    app: AppHandle,
    request: CreateBackupRequest,
) -> Result<BackupSummary, IpcError> {
    let archive_path = request.archive_path.clone();
    let note = request.note.clone();
    with_backup_service(&app, move |backup_service, workspace_service| {
        let status = workspace_service.get_status()?;
        let workspace = status.workspace.ok_or_else(|| {
            AppError::Persistence(crate::ports::persistence::PersistenceError::new(
                "read workspace for backup",
                std::io::Error::other("workspace is not initialized"),
            ))
        })?;
        backup_service.create_backup(
            archive_path,
            workspace.workspace_name,
            workspace.teacher_name,
            status.database.schema_version,
            status.storage.asset_count,
            status.storage.total_bytes,
            note,
        )
    })
    .await
}

#[tauri::command]
pub async fn backup_v1_preview_restore(
    app: AppHandle,
    request: PreviewRestoreRequest,
) -> Result<RestorePreview, IpcError> {
    let archive_path = request.archive_path.clone();
    with_backup_service(&app, move |backup_service, _workspace_service| {
        backup_service.preview_restore(archive_path)
    })
    .await
}

#[tauri::command]
pub async fn backup_v1_restore(
    app: AppHandle,
    request: RestoreBackupRequest,
) -> Result<RestoreSummary, IpcError> {
    let archive_path = request.archive_path.clone();
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let backup_service = app.state::<BackupService>();
        let mut summary = backup_service.restore_backup(archive_path)?;
        // 恢复成功后尝试热重载所有服务的数据库连接。
        // 重载成功则无需重启，重载失败则保留 requires_restart=true 以便提示用户重启。
        let reloader = app.state::<ServiceReloader>();
        match reloader.reload_all(&app) {
            Ok(()) => summary.requires_restart = false,
            Err(_) => summary.requires_restart = true,
        }
        Ok::<RestoreSummary, AppError>(summary)
    })
    .await
    .map_err(|error| {
        eprintln!("backup native task failed: {error}");
        IpcError::task_failed()
    })?
    .map_err(IpcError::from)
}

async fn with_backup_service<T, F>(app: &AppHandle, operation: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce(&BackupService, &WorkspaceService) -> Result<T, AppError> + Send + 'static,
{
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let backup_service = app.state::<BackupService>();
        let workspace_service = app.state::<WorkspaceService>();
        operation(backup_service.inner(), workspace_service.inner())
    })
    .await
    .map_err(|error| {
        eprintln!("backup native task failed: {error}");
        IpcError::task_failed()
    })?
    .map_err(IpcError::from)
}
