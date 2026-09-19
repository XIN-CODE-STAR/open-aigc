//! 插件中心 IPC：技能（SKILL.md）与 MCP 服务器的发现 / 导入 / 下载 / 管理。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::application::{
    error::AppError,
    mcp_client,
    plugin_service::{self, McpScanCandidate, McpServerConfig, SkillCandidate, SkillMeta},
};
use crate::ipc::error::IpcError;

/// 工作区目录（与 workspace_v1_get_root_path 一致：app_local_data_dir/workspace）。
fn workspace_dir(app: &AppHandle) -> Result<PathBuf, IpcError> {
    app.path()
        .app_local_data_dir()
        .map(|dir| dir.join("workspace"))
        .map_err(|e| IpcError::from(AppError::new("resolve workspace dir", e)))
}

/// 阻塞操作放到线程池执行（下载 / 进程探测可能耗时数十秒）。
async fn on_workspace<T, F>(app: AppHandle, operation: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce(&Path) -> Result<T, AppError> + Send + 'static,
{
    let dir = workspace_dir(&app)?;
    let result = tauri::async_runtime::spawn_blocking(move || operation(&dir))
        .await
        .map_err(|error| {
            eprintln!("plugins native task failed: {error}");
            IpcError::task_failed()
        })?;
    result.map_err(IpcError::from)
}

// ──────────────────────────────────────────────────────────────────
// 技能
// ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn skill_v1_list(app: AppHandle) -> Result<Vec<SkillMeta>, IpcError> {
    on_workspace(app, plugin_service::list_skills).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetSkillBodyRequest {
    pub slug: String,
}

#[tauri::command]
pub async fn skill_v1_get_body(
    app: AppHandle,
    request: GetSkillBodyRequest,
) -> Result<String, IpcError> {
    let slug = request.slug;
    on_workspace(app, move |dir| plugin_service::get_skill_body(dir, &slug)).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteSkillRequest {
    pub slug: String,
}

#[tauri::command]
pub async fn skill_v1_delete(app: AppHandle, request: DeleteSkillRequest) -> Result<(), IpcError> {
    let slug = request.slug;
    on_workspace(app, move |dir| plugin_service::delete_skill(dir, &slug)).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScanLocalSkillsRequest {
    /// 用户通过目录选择对话框选定的本地目录
    pub dir: String,
}

#[tauri::command]
pub async fn skill_v1_scan_local(
    request: ScanLocalSkillsRequest,
) -> Result<Vec<SkillCandidate>, IpcError> {
    let dir = request.dir;
    let result =
        tauri::async_runtime::spawn_blocking(move || plugin_service::scan_local_skills(&dir))
            .await
            .map_err(|error| {
                eprintln!("skill scan failed: {error}");
                IpcError::task_failed()
            })?;
    result.map_err(IpcError::from)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportSkillRequest {
    /// 技能目录或单个 .md 文件的绝对路径
    pub path: String,
}

#[tauri::command]
pub async fn skill_v1_import(
    app: AppHandle,
    request: ImportSkillRequest,
) -> Result<SkillMeta, IpcError> {
    let path = request.path;
    on_workspace(app, move |dir| {
        plugin_service::import_skill_from_path(dir, &path, "local-import")
    })
    .await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadSkillRequest {
    /// .md 直链 / zip 归档 / GitHub 仓库页
    pub url: String,
}

#[tauri::command]
pub async fn skill_v1_download(
    app: AppHandle,
    request: DownloadSkillRequest,
) -> Result<SkillMeta, IpcError> {
    let url = request.url;
    on_workspace(app, move |dir| plugin_service::download_skill(dir, &url)).await
}

// ──────────────────────────────────────────────────────────────────
// MCP 服务器
// ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn mcp_v1_list(app: AppHandle) -> Result<Vec<McpServerConfig>, IpcError> {
    on_workspace(app, plugin_service::list_mcp_servers).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AddMcpServerRequest {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[tauri::command]
pub async fn mcp_v1_add(
    app: AppHandle,
    request: AddMcpServerRequest,
) -> Result<McpServerConfig, IpcError> {
    on_workspace(app, move |dir| {
        plugin_service::add_mcp_server(
            dir,
            &request.name,
            &request.command,
            &request.args,
            &request.env,
            request.enabled,
        )
    })
    .await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateMcpServerRequest {
    pub server: McpServerConfig,
}

#[tauri::command]
pub async fn mcp_v1_update(
    app: AppHandle,
    request: UpdateMcpServerRequest,
) -> Result<(), IpcError> {
    let server = request.server;
    on_workspace(app, move |dir| {
        plugin_service::update_mcp_server(dir, &server)
    })
    .await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoveMcpServerRequest {
    pub id: String,
}

#[tauri::command]
pub async fn mcp_v1_remove(
    app: AppHandle,
    request: RemoveMcpServerRequest,
) -> Result<(), IpcError> {
    let id = request.id;
    on_workspace(app, move |dir| plugin_service::remove_mcp_server(dir, &id)).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToggleMcpServerRequest {
    pub id: String,
    pub enabled: bool,
}

#[tauri::command]
pub async fn mcp_v1_toggle(
    app: AppHandle,
    request: ToggleMcpServerRequest,
) -> Result<(), IpcError> {
    let (id, enabled) = (request.id, request.enabled);
    on_workspace(app, move |dir| {
        plugin_service::set_mcp_server_enabled(dir, &id, enabled)
    })
    .await
}

#[tauri::command]
pub async fn mcp_v1_scan_local() -> Result<Vec<McpScanCandidate>, IpcError> {
    let result = tauri::async_runtime::spawn_blocking(plugin_service::scan_local_mcp_configs)
        .await
        .map_err(|error| {
            eprintln!("mcp scan failed: {error}");
            IpcError::task_failed()
        })?;
    Ok(result)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeMcpServerRequest {
    pub id: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpProbeResult {
    pub tools: Vec<mcp_client::McpToolInfo>,
}

#[tauri::command]
pub async fn mcp_v1_probe(
    app: AppHandle,
    request: ProbeMcpServerRequest,
) -> Result<McpProbeResult, IpcError> {
    let dir = workspace_dir(&app)?;
    let id = request.id;
    let spawn =
        tauri::async_runtime::spawn_blocking(move || -> Result<McpProbeResult, AppError> {
            let servers = plugin_service::list_mcp_servers(&dir)?;
            let config = servers.into_iter().find(|s| s.id == id).ok_or_else(|| {
                AppError::Persistence(crate::ports::persistence::PersistenceError::new(
                    "probe mcp server",
                    std::io::Error::new(std::io::ErrorKind::NotFound, "服务器不存在"),
                ))
            })?;
            let tools = mcp_client::list_tools(&config)?;
            Ok(McpProbeResult { tools })
        });
    let result = spawn.await.map_err(|error| {
        eprintln!("mcp probe failed: {error}");
        IpcError::task_failed()
    })?;
    result.map_err(IpcError::from)
}
