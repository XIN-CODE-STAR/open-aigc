use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::{
    application::{credential_service::CredentialService, error::AppError},
    domain::credentials::{CredentialDraft, CredentialRecord, CredentialType},
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
    /// 单一 API Key（api_key 类型必填；access_secret 类型留空）。
    #[serde(default)]
    pub api_key: String,
    /// AccessSecret 类型的 Access Key（AK）。
    #[serde(default)]
    pub access_key: Option<String>,
    /// AccessSecret 类型的 Secret Key（SK）。
    #[serde(default)]
    pub secret_key: Option<String>,
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
    /// AccessSecret 类型的 Access Key（AK）。与 secret_key 同时提供时按 AK/SK 覆盖密钥。
    #[serde(default)]
    pub access_key: Option<String>,
    /// AccessSecret 类型的 Secret Key（SK）。
    #[serde(default)]
    pub secret_key: Option<String>,
}

/// 组装写入 keychain 的密钥载荷。
///
/// AccessSecret（AK/SK）序列化为 JSON `{"access_key","secret_key"}`，与
/// `CredentialManager::build_context` 的解析约定一致；其余类型原样使用单一密钥字符串。
fn build_secret_payload(
    cred_type: CredentialType,
    api_key: String,
    access_key: Option<String>,
    secret_key: Option<String>,
) -> String {
    if cred_type == CredentialType::AccessSecret {
        access_secret_payload(
            access_key.unwrap_or_default(),
            secret_key.unwrap_or_default(),
        )
    } else {
        api_key
    }
}

fn access_secret_payload(access_key: String, secret_key: String) -> String {
    serde_json::json!({ "access_key": access_key, "secret_key": secret_key }).to_string()
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
        let cred_type = CredentialType::parse(&request.credential_type);
        let draft = CredentialDraft::try_new_with_type(
            request.provider_name,
            request.display_name,
            request.base_url,
            request.model_name,
            cred_type,
        )?;
        let secret = build_secret_payload(
            cred_type,
            request.api_key,
            request.access_key,
            request.secret_key,
        );
        service.create(draft, secret)
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
        // AK/SK 任一存在即按 AccessSecret 组装 JSON 覆盖密钥；否则沿用单一密钥
        let secret = if request.access_key.is_some() || request.secret_key.is_some() {
            Some(access_secret_payload(
                request.access_key.unwrap_or_default(),
                request.secret_key.unwrap_or_default(),
            ))
        } else {
            request.api_key
        };
        service.update(&request.id, draft, secret)
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
