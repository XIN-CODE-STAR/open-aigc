use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::{
    application::{credential_service::CredentialService, error::AppError},
    domain::credentials::{CredentialDraft, CredentialRecord},
    ipc::error::IpcError,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListCredentialsRequest {}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateCredentialRequest {
    pub provider_name: String,
    pub display_name: String,
    pub base_url: String,
    pub model_name: String,
    pub api_key: String,
    /// 认证类型（可选，默认 api_key。账号类传 session_cookie / browser_session）
    #[serde(default)]
    pub credential_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateCredentialRequest {
    pub id: String,
    pub provider_name: String,
    pub display_name: String,
    pub base_url: String,
    pub model_name: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteCredentialRequest {
    pub id: String,
}

#[tauri::command]
pub async fn credential_v1_list(
    app: AppHandle,
    _request: ListCredentialsRequest,
) -> Result<Vec<CredentialRecord>, IpcError> {
    with_credential_service(app, |service| service.list()).await
}

#[tauri::command]
pub async fn credential_v1_create(
    app: AppHandle,
    request: CreateCredentialRequest,
) -> Result<CredentialRecord, IpcError> {
    with_credential_service(app, move |service| {
        use crate::domain::credentials::CredentialType;
        let cred_type = CredentialType::parse(&request.credential_type);
        let draft = CredentialDraft::try_new_with_type(
            request.provider_name,
            request.display_name,
            request.base_url,
            request.model_name,
            cred_type,
        )?;
        service.create(draft, request.api_key)
    })
    .await
}

#[tauri::command]
pub async fn credential_v1_update(
    app: AppHandle,
    request: UpdateCredentialRequest,
) -> Result<CredentialRecord, IpcError> {
    with_credential_service(app, move |service| {
        let draft = CredentialDraft::try_new(
            request.provider_name,
            request.display_name,
            request.base_url,
            request.model_name,
        )?;
        service.update(&request.id, draft, request.api_key)
    })
    .await
}

#[tauri::command]
pub async fn credential_v1_delete(
    app: AppHandle,
    request: DeleteCredentialRequest,
) -> Result<(), IpcError> {
    with_credential_service(app, move |service| service.delete(&request.id)).await
}

async fn with_credential_service<T, F>(app: AppHandle, operation: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce(&CredentialService) -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let service = app.state::<CredentialService>();
        operation(service.inner())
    })
    .await
    .map_err(|error| {
        eprintln!("credential native task failed: {error}");
        IpcError::task_failed()
    })?
    .map_err(IpcError::from)
}
