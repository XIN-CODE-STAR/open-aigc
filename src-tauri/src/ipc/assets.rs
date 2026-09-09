use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::{
    application::{asset_service::AssetService, error::AppError},
    domain::assets::{AssetImportSummary, AssetRecord, AssetReverificationSummary},
    ipc::error::IpcError,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListAssetsRequest {
    pub search: Option<String>,
    pub storage_namespace: Option<String>,
    pub asset_kind: Option<String>,
    pub integrity_status: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAssetRequest {
    pub id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportAssetsRequest {
    pub source_paths: Vec<String>,
    pub namespace: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReverifyAssetsRequest {}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenAssetRequest {
    pub id: String,
}

#[tauri::command]
pub async fn asset_v1_list(
    app: AppHandle,
    request: ListAssetsRequest,
) -> Result<Vec<AssetRecord>, IpcError> {
    with_asset_service_read(app, move |service| {
        service.list(
            request.search,
            request.storage_namespace,
            request.asset_kind,
            request.integrity_status,
            request.limit,
        )
    })
    .await
}

#[tauri::command]
pub async fn asset_v1_get(
    app: AppHandle,
    request: GetAssetRequest,
) -> Result<Option<AssetRecord>, IpcError> {
    with_asset_service_read(app, move |service| service.get(&request.id)).await
}

#[tauri::command]
pub async fn asset_v1_import(
    app: AppHandle,
    request: ImportAssetsRequest,
) -> Result<AssetImportSummary, IpcError> {
    with_asset_service_write(app, move |service| {
        service.import(request.source_paths, request.namespace)
    })
    .await
}

#[tauri::command]
pub async fn asset_v1_reverify(
    app: AppHandle,
    _request: ReverifyAssetsRequest,
) -> Result<AssetReverificationSummary, IpcError> {
    with_asset_service_write(app, move |service| service.reverify()).await
}

#[tauri::command]
pub async fn asset_v1_delete(app: AppHandle, request: OpenAssetRequest) -> Result<(), IpcError> {
    with_asset_service_write(app, move |service| service.delete(&request.id)).await
}

#[tauri::command]
pub async fn asset_v1_open_file(app: AppHandle, request: OpenAssetRequest) -> Result<(), IpcError> {
    // 克隆一份给闭包内调用 open_file 使用；原 app 进入 helper 的 spawn_blocking。
    let app_for_closure = app.clone();
    with_asset_service_read(app, move |service| {
        service.open_file(&app_for_closure, &request.id)
    })
    .await
}

#[tauri::command]
pub async fn asset_v1_open_containing_folder(
    app: AppHandle,
    request: OpenAssetRequest,
) -> Result<(), IpcError> {
    let app_for_closure = app.clone();
    with_asset_service_read(app, move |service| {
        service.open_containing_folder(&app_for_closure, &request.id)
    })
    .await
}

async fn with_asset_service_read<T, F>(app: AppHandle, operation: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce(&AssetService) -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let service = app.state::<AssetService>();
        operation(service.inner())
    })
    .await
    .map_err(|error| {
        eprintln!("asset native task failed: {error}");
        IpcError::task_failed()
    })?
    .map_err(IpcError::from)
}

async fn with_asset_service_write<T, F>(app: AppHandle, operation: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce(&AssetService) -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let service = app.state::<AssetService>();
        operation(service.inner())
    })
    .await
    .map_err(|error| {
        eprintln!("asset native task failed: {error}");
        IpcError::task_failed()
    })?
    .map_err(IpcError::from)
}
