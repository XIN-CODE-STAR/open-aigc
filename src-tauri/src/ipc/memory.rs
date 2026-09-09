use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::{
    application::{error::AppError, memory_service::MemoryServiceImpl},
    ipc::error::IpcError,
    ports::memory_service::MemoryServicePort,
};

/// 记忆服务状态。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStatusResponse {
    pub available: bool,
    pub status: String,
}

/// 记忆搜索结果项。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySearchItem {
    pub source_type: String,
    pub content: String,
    pub score: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryStatusRequest {}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemorySearchRequest {
    pub query: String,
    pub limit: Option<usize>,
}

#[tauri::command]
pub async fn memory_v1_status(
    app: AppHandle,
    _request: MemoryStatusRequest,
) -> Result<MemoryStatusResponse, IpcError> {
    tauri::async_runtime::spawn_blocking(move || {
        let memory = app.state::<MemoryServiceImpl>();
        let available = memory.is_available();
        let status = if available {
            "active".to_owned()
        } else {
            "inactive".to_owned()
        };
        Ok(MemoryStatusResponse { available, status })
    })
    .await
    .map_err(|_| IpcError::from(AppError::StateUnavailable))?
}

#[tauri::command]
pub async fn memory_v1_search(
    app: AppHandle,
    request: MemorySearchRequest,
) -> Result<Vec<MemorySearchItem>, IpcError> {
    tauri::async_runtime::spawn_blocking(move || {
        let memory = app.state::<MemoryServiceImpl>();
        let limit = request.limit.unwrap_or(10);
        let results = memory
            .recall("default", &request.query, limit)
            .map_err(IpcError::from)?;

        Ok(results
            .into_iter()
            .map(|r| MemorySearchItem {
                source_type: r.source_type,
                content: r.content,
                score: r.score,
            })
            .collect())
    })
    .await
    .map_err(|_| IpcError::from(AppError::StateUnavailable))?
}
