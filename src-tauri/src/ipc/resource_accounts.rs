//! 资源账号 IPC 命令。

use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::{
    application::{error::AppError, resource_account_service::ResourceAccountService},
    ipc::error::IpcError,
    ports::resource_account_repository::{ResourceAccountDraft, ResourceAccountUpdate},
    ports::resource_connector::ResourceAccountRecord,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListResourceAccountsRequest {}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateResourceAccountRequest {
    pub provider_id: String,
    #[serde(default = "default_account_type")]
    pub account_type: String,
    pub display_name: String,
    #[serde(default)]
    pub base_url: String,
    /// Session ID / Cookie 值（写入 Keychain）。
    pub session_secret: String,
    #[serde(default = "default_extra_json")]
    pub extra_json: String,
}

fn default_account_type() -> String {
    "session_cookie".to_owned()
}

fn default_extra_json() -> String {
    "{}".to_owned()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateResourceAccountRequest {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default = "default_extra_json")]
    pub extra_json: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateSessionRequest {
    pub id: String,
    pub session_secret: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetAccountEnabledRequest {
    pub id: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteResourceAccountRequest {
    pub id: String,
}

// ── Commands ──

#[tauri::command]
pub async fn resource_account_v1_list(
    app: AppHandle,
    _request: ListResourceAccountsRequest,
) -> Result<Vec<ResourceAccountRecord>, IpcError> {
    with_service(app, |service| service.list()).await
}

#[tauri::command]
pub async fn resource_account_v1_create(
    app: AppHandle,
    request: CreateResourceAccountRequest,
) -> Result<ResourceAccountRecord, IpcError> {
    with_service(app, move |service| {
        let draft = ResourceAccountDraft {
            provider_id: request.provider_id,
            account_type: request.account_type,
            display_name: request.display_name,
            base_url: request.base_url,
            extra_json: request.extra_json,
        };
        service.create(draft, request.session_secret)
    })
    .await
}

#[tauri::command]
pub async fn resource_account_v1_update(
    app: AppHandle,
    request: UpdateResourceAccountRequest,
) -> Result<ResourceAccountRecord, IpcError> {
    with_service(app, move |service| {
        let update = ResourceAccountUpdate {
            display_name: request.display_name,
            base_url: request.base_url,
            extra_json: request.extra_json,
        };
        service.update(&request.id, update)
    })
    .await
}

#[tauri::command]
pub async fn resource_account_v1_update_session(
    app: AppHandle,
    request: UpdateSessionRequest,
) -> Result<(), IpcError> {
    with_service(app, move |service| {
        service.update_session(&request.id, request.session_secret)
    })
    .await
}

#[tauri::command]
pub async fn resource_account_v1_set_enabled(
    app: AppHandle,
    request: SetAccountEnabledRequest,
) -> Result<(), IpcError> {
    with_service(app, move |service| {
        service.set_enabled(&request.id, request.enabled)
    })
    .await
}

#[tauri::command]
pub async fn resource_account_v1_delete(
    app: AppHandle,
    request: DeleteResourceAccountRequest,
) -> Result<(), IpcError> {
    with_service(app, move |service| service.delete(&request.id)).await
}

// ── Helper ──

async fn with_service<T, F>(app: AppHandle, operation: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce(&ResourceAccountService) -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let service = app.state::<ResourceAccountService>();
        operation(service.inner())
    })
    .await
    .map_err(|error| {
        eprintln!("resource account native task failed: {error}");
        IpcError::task_failed()
    })?
    .map_err(IpcError::from)
}
