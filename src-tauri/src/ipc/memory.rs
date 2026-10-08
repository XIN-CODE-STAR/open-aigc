use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::{
    application::{error::AppError, memory_service::MemoryServiceImpl},
    ipc::error::IpcError,
    ports::memory_service::MemoryServicePort,
};

/// 记忆服务状态（含 EverOS 长期记忆配置）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStatusResponse {
    pub available: bool,
    pub status: String,
    /// EverOS 是否已启用。
    pub enabled: bool,
    /// EverOS 子进程是否在运行。
    pub running: bool,
    /// EverOS 服务端口。
    pub port: u16,
    /// 记忆根目录。
    pub root_path: String,
    /// 根目录下是否存在 everos.toml（缺失需先运行 scripts/setup-everos.ps1）。
    pub config_present: bool,
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

/// 启用 / 停用 EverOS 长期记忆。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EverosSetEnabledRequest {
    pub enabled: bool,
    /// 可选：覆盖记忆根目录（启用时生效）。
    pub root_path: Option<String>,
}

/// 将 EverOS 状态映射为 IPC 响应。
fn to_response(everos: crate::application::memory_service::EverosStatus) -> MemoryStatusResponse {
    let status = if everos.available {
        "active"
    } else if everos.enabled {
        "enabled_but_unavailable"
    } else {
        "inactive"
    };
    MemoryStatusResponse {
        available: everos.available,
        status: status.to_owned(),
        enabled: everos.enabled,
        running: everos.running,
        port: everos.port,
        root_path: everos.root_path,
        config_present: everos.config_present,
    }
}

#[tauri::command]
pub async fn memory_v1_status(
    app: AppHandle,
    _request: MemoryStatusRequest,
) -> Result<MemoryStatusResponse, IpcError> {
    tauri::async_runtime::spawn_blocking(move || {
        let memory = app.state::<MemoryServiceImpl>();
        Ok(to_response(memory.status()))
    })
    .await
    .map_err(|_| IpcError::from(AppError::StateUnavailable))?
}

#[tauri::command]
pub async fn memory_v1_everos_set_enabled(
    app: AppHandle,
    request: EverosSetEnabledRequest,
) -> Result<MemoryStatusResponse, IpcError> {
    tauri::async_runtime::spawn_blocking(move || {
        let memory = app.state::<MemoryServiceImpl>();
        let everos = memory
            .set_enabled(request.enabled, request.root_path)
            .map_err(IpcError::from)?;
        Ok(to_response(everos))
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
