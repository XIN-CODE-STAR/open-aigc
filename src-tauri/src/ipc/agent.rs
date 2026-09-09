use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::{
    application::{
        agent_service::{AgentService, SendMessageResult},
        error::AppError,
    },
    domain::agent::{ConversationRecord, MessageRecord, ToolInvocationRecord},
    ipc::error::IpcError,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateConversationRequest {
    pub title: String,
    pub credential_id: String,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListConversationsRequest {}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetConversationRequest {
    pub id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteConversationRequest {
    pub id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenameConversationRequest {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListMessagesRequest {
    pub conversation_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SendMessageRequest {
    pub conversation_id: String,
    pub content: String,
    pub attachments: Option<Vec<AttachmentInput>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttachmentInput {
    pub name: String,
    pub mime_type: String,
    pub data_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebugLogRequest {
    pub message: String,
}

/// 前端诊断日志通道：WebView2 console 不会转发到 dev 日志，
/// 前端通过该命令把关键诊断信息（如事件订阅状态）写入后端 stderr。
#[tauri::command]
pub async fn agent_v1_debug_log(request: DebugLogRequest) -> Result<(), IpcError> {
    eprintln!("[Frontend] {}", request.message);
    Ok(())
}

#[tauri::command]
pub async fn agent_v1_create_conversation(
    app: AppHandle,
    request: CreateConversationRequest,
) -> Result<ConversationRecord, IpcError> {
    let app_for_op = app.clone();
    with_agent_service_write(app, move |service| {
        let workspace_id = current_workspace_id(&app_for_op)?;
        service.create_conversation(
            workspace_id,
            request.title,
            request.credential_id,
            request.system_prompt,
        )
    })
    .await
}

#[tauri::command]
pub async fn agent_v1_list_conversations(
    app: AppHandle,
    _request: ListConversationsRequest,
) -> Result<Vec<ConversationRecord>, IpcError> {
    let app_for_op = app.clone();
    with_agent_service_read(app, move |service| {
        let workspace_id = current_workspace_id(&app_for_op)?;
        service.list_conversations(&workspace_id)
    })
    .await
}

#[tauri::command]
pub async fn agent_v1_get_conversation(
    app: AppHandle,
    request: GetConversationRequest,
) -> Result<Option<ConversationRecord>, IpcError> {
    with_agent_service_read(app, move |service| service.get_conversation(&request.id)).await
}

#[tauri::command]
pub async fn agent_v1_delete_conversation(
    app: AppHandle,
    request: DeleteConversationRequest,
) -> Result<(), IpcError> {
    with_agent_service_write(app, move |service| service.delete_conversation(&request.id)).await
}

#[tauri::command]
pub async fn agent_v1_rename_conversation(
    app: AppHandle,
    request: RenameConversationRequest,
) -> Result<ConversationRecord, IpcError> {
    with_agent_service_write(app, move |service| {
        service.rename_conversation(&request.id, request.title)
    })
    .await
}

#[tauri::command]
pub async fn agent_v1_list_messages(
    app: AppHandle,
    request: ListMessagesRequest,
) -> Result<Vec<MessageRecord>, IpcError> {
    with_agent_service_read(app, move |service| {
        service.list_messages(&request.conversation_id)
    })
    .await
}

#[tauri::command]
pub async fn agent_v1_list_invocations(
    app: AppHandle,
    request: ListMessagesRequest,
) -> Result<Vec<ToolInvocationRecord>, IpcError> {
    with_agent_service_read(app, move |service| {
        service.list_invocations(&request.conversation_id)
    })
    .await
}

#[tauri::command]
pub async fn agent_v1_send_message(
    app: AppHandle,
    request: SendMessageRequest,
) -> Result<SendMessageResult, IpcError> {
    let app_for_op = app.clone();
    let attachments = request.attachments.unwrap_or_default();
    let image_data_urls: Vec<String> = attachments.into_iter().map(|a| a.data_url).collect();
    with_agent_service_write(app, move |service| {
        service.send_message(
            &app_for_op,
            &request.conversation_id,
            request.content,
            image_data_urls,
        )
    })
    .await
}

async fn with_agent_service_read<T, F>(app: AppHandle, operation: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce(&AgentService) -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let service = app.state::<AgentService>();
        operation(service.inner())
    })
    .await
    .map_err(|error| {
        eprintln!("agent native task failed: {error}");
        IpcError::task_failed()
    })?
    .map_err(IpcError::from)
}

async fn with_agent_service_write<T, F>(app: AppHandle, operation: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce(&AgentService) -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let service = app.state::<AgentService>();
        operation(service.inner())
    })
    .await
    .map_err(|error| {
        eprintln!("agent native task failed: {error}");
        IpcError::task_failed()
    })?
    .map_err(IpcError::from)
}

/// 从 WorkspaceService 读取当前工作空间 ID。
fn current_workspace_id(app: &AppHandle) -> Result<String, AppError> {
    use crate::application::workspace_service::WorkspaceService;
    let workspace_service = app.state::<WorkspaceService>();
    let status = workspace_service.get_status()?;
    status
        .workspace
        .map(|w| w.workspace_id)
        .ok_or(AppError::AgentValidation(
            crate::domain::agent::AgentValidationError::Required {
                field: "workspaceId",
            },
        ))
}

// ── 设置生成输出目录 ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetOutputDirectoryRequest {
    /// 目录路径，为空则清除（恢复默认）。
    pub path: Option<String>,
}

#[tauri::command]
pub async fn agent_v1_set_output_directory(
    app: AppHandle,
    request: SetOutputDirectoryRequest,
) -> Result<(), IpcError> {
    let path = request.path.map(std::path::PathBuf::from);
    let service = app.state::<AgentService>();
    eprintln!("[Agent] set_output_directory: {:?}", path);
    service.set_output_directory(path);
    Ok(())
}

// ── Semantic Pipeline — 图片语义分析 ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalyzeAssetRequest {
    /// 资源 ID。
    pub asset_id: String,
    /// 图片 data URL（"data:image/...;base64,..."）。
    pub image_data_url: String,
    /// 首选适配器（可选，如 "dashscope-qwen-vl" / "florence-2"）。
    pub preferred_adapter: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchAnalyzeRequest {
    /// 资源列表：[(asset_id, image_data_url)]。
    pub items: Vec<(String, String)>,
    /// 首选适配器。
    pub preferred_adapter: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticSearchRequest {
    /// 搜索关键词。
    pub query: String,
    /// 最大结果数。
    pub limit: Option<usize>,
}

/// 分析单个资源的图片，生成语义画像。
#[tauri::command]
pub async fn agent_v1_analyze_asset(
    app: AppHandle,
    request: AnalyzeAssetRequest,
) -> Result<crate::application::semantic::pipeline_service::AnalysisResult, IpcError> {
    let service =
        app.state::<crate::application::semantic::pipeline_service::SemanticPipelineService>();
    service
        .analyze_asset(
            &request.asset_id,
            &request.image_data_url,
            request.preferred_adapter.as_deref(),
        )
        .map_err(IpcError::from)
}

/// 批量分析资源图片。
#[tauri::command]
pub async fn agent_v1_analyze_assets_batch(
    app: AppHandle,
    request: BatchAnalyzeRequest,
) -> Result<Vec<crate::application::semantic::pipeline_service::AnalysisResult>, IpcError> {
    let service =
        app.state::<crate::application::semantic::pipeline_service::SemanticPipelineService>();
    service
        .analyze_batch(&request.items, request.preferred_adapter.as_deref(), None)
        .map_err(IpcError::from)
}

/// 按语义搜索资源。
#[tauri::command]
pub async fn agent_v1_search_assets_semantic(
    app: AppHandle,
    request: SemanticSearchRequest,
) -> Result<Vec<crate::application::semantic::retrieval_service::RetrievalResult>, IpcError> {
    let service =
        app.state::<crate::application::semantic::retrieval_service::SemanticRetrievalService>();
    let limit = request.limit.unwrap_or(10);
    service
        .unified_search(&request.query, limit)
        .map_err(IpcError::from)
}
