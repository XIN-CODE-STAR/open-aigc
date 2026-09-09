#![allow(dead_code)]
use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::{
    application::{
        error::AppError, generation_queue_service::GenerationQueueService,
        generation_submit_service::GenerationSubmitService,
    },
    domain::generation::GenerationAttemptRecord,
    ipc::error::IpcError,
};

// ── Helper ──

async fn with_queue<T, F>(app: AppHandle, operation: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce(&GenerationQueueService) -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let service = app.state::<GenerationQueueService>();
        operation(service.inner())
    })
    .await
    .map_err(|e| {
        eprintln!("generation queue task failed: {e}");
        IpcError::task_failed()
    })?
    .map_err(IpcError::from)
}

async fn with_submit<T, F>(app: AppHandle, operation: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce(&GenerationSubmitService) -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let service = app.state::<GenerationSubmitService>();
        operation(service.inner())
    })
    .await
    .map_err(|e| {
        eprintln!("generation submit task failed: {e}");
        IpcError::task_failed()
    })?
    .map_err(IpcError::from)
}

// ── Submit ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitAttemptRequest {
    pub task_id: String,
    pub credential_id: String,
    pub capability: String,
    pub request_snapshot_json: String,
    pub provider_id: String,
}

#[tauri::command]
pub async fn queue_v1_submit_attempt(
    app: AppHandle,
    request: SubmitAttemptRequest,
) -> Result<GenerationAttemptRecord, IpcError> {
    let tid = request.task_id.clone();
    let cid = request.credential_id.clone();
    let cap = request.capability.clone();
    let snap = request.request_snapshot_json.clone();
    let pid = request.provider_id.clone();
    // 使用 GenerationSubmitService（真正调用 Provider），传入 app 用于进度事件
    with_submit(app.clone(), move |s| {
        s.submit_and_dispatch(&tid, &cid, &cap, &snap, &pid, Some(&app))
    })
    .await
}

// ── Get ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAttemptRequest {
    pub id: String,
}

#[tauri::command]
pub async fn queue_v1_get_attempt(
    app: AppHandle,
    request: GetAttemptRequest,
) -> Result<Option<GenerationAttemptRecord>, IpcError> {
    let id = request.id.clone();
    with_queue(app, move |s| s.get_attempt(&id)).await
}

// ── List by task ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListAttemptsRequest {
    pub task_id: String,
}

#[tauri::command]
pub async fn queue_v1_list_attempts(
    app: AppHandle,
    request: ListAttemptsRequest,
) -> Result<Vec<GenerationAttemptRecord>, IpcError> {
    let tid = request.task_id.clone();
    with_queue(app, move |s| s.list_by_task(&tid)).await
}

// ── List active ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListActiveAttemptsRequest {}

#[tauri::command]
pub async fn queue_v1_list_active(
    app: AppHandle,
    _request: ListActiveAttemptsRequest,
) -> Result<Vec<GenerationAttemptRecord>, IpcError> {
    with_queue(app, |s| s.list_active()).await
}

// ── Cancel ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelAttemptRequest {
    pub id: String,
}

#[tauri::command]
pub async fn queue_v1_cancel_attempt(
    app: AppHandle,
    request: CancelAttemptRequest,
) -> Result<GenerationAttemptRecord, IpcError> {
    let id = request.id.clone();
    with_queue(app, move |s| s.cancel(&id)).await
}

// ── Retry ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetryAttemptRequest {
    pub id: String,
}

#[tauri::command]
pub async fn queue_v1_retry_attempt(
    app: AppHandle,
    request: RetryAttemptRequest,
) -> Result<GenerationAttemptRecord, IpcError> {
    let id = request.id.clone();
    with_queue(app, move |s| s.retry(&id)).await
}
