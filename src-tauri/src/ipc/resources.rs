use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::{
    application::{error::AppError, resource_service::ResourceService},
    domain::resources::AssociationRecord,
    ipc::error::IpcError,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListAssociationsRequest {
    pub asset_id: Option<String>,
    pub context_kind: Option<String>,
    pub context_ref: Option<String>,
    pub role: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAssociationRequest {
    pub id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateAssociationRequest {
    pub asset_id: String,
    pub context_kind: String,
    pub context_ref: String,
    pub role: String,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteAssociationRequest {
    pub id: String,
}

#[tauri::command]
pub async fn resource_v1_list(
    app: AppHandle,
    request: ListAssociationsRequest,
) -> Result<Vec<AssociationRecord>, IpcError> {
    with_resource_service(app, move |service| {
        service.list(
            request.asset_id,
            request.context_kind,
            request.context_ref,
            request.role,
            request.limit,
        )
    })
    .await
}

#[tauri::command]
pub async fn resource_v1_get(
    app: AppHandle,
    request: GetAssociationRequest,
) -> Result<Option<AssociationRecord>, IpcError> {
    with_resource_service(app, move |service| service.get(&request.id)).await
}

#[tauri::command]
pub async fn resource_v1_create(
    app: AppHandle,
    request: CreateAssociationRequest,
) -> Result<AssociationRecord, IpcError> {
    with_resource_service(app, move |service| {
        service.create(
            request.asset_id,
            request.context_kind,
            request.context_ref,
            request.role,
            request.notes,
        )
    })
    .await
}

#[tauri::command]
pub async fn resource_v1_delete(
    app: AppHandle,
    request: DeleteAssociationRequest,
) -> Result<(), IpcError> {
    with_resource_service(app, move |service| service.delete(&request.id)).await
}

async fn with_resource_service<T, F>(app: AppHandle, operation: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce(&ResourceService) -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let service = app.state::<ResourceService>();
        operation(service.inner())
    })
    .await
    .map_err(|error| {
        eprintln!("resource native task failed: {error}");
        IpcError::task_failed()
    })?
    .map_err(IpcError::from)
}
