use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::{
    application::{
        error::AppError, generation_service::GenerationService, workspace_service::WorkspaceService,
    },
    domain::generation::{GenerationResultRecord, GenerationTaskRecord},
    ipc::error::IpcError,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitTaskRequest {
    pub provider_name: String,
    pub model_name: String,
    pub prompt_text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordOutputRequest {
    pub task_id: String,
    pub source_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkFailedRequest {
    pub task_id: String,
    pub error_message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListTasksRequest {
    pub status: Option<String>,
    pub provider_name: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetTaskRequest {
    pub id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListResultsRequest {
    pub task_id: String,
}

#[tauri::command]
pub async fn generation_v1_submit(
    app: AppHandle,
    request: SubmitTaskRequest,
) -> Result<GenerationTaskRecord, IpcError> {
    with_generation_service_write(app, move |service, workspace_service| {
        let status = workspace_service.get_status()?;
        let workspace_id =
            status
                .workspace
                .map(|w| w.workspace_id)
                .ok_or(AppError::GenerationValidation(
                    crate::domain::generation::GenerationValidationError::Required {
                        field: "workspaceId",
                    },
                ))?;
        service.submit_task(
            workspace_id,
            request.provider_name,
            request.model_name,
            request.prompt_text,
        )
    })
    .await
}

#[tauri::command]
pub async fn generation_v1_record_output(
    app: AppHandle,
    request: RecordOutputRequest,
) -> Result<GenerationResultRecord, IpcError> {
    let task_id = request.task_id;
    let image_url = request.source_path.clone();
    // 画布写回钩子（异步路径）用：任务若来自 Agent 画布（挂过 pending 占位节点），
    // 按 taskId 定位并补全图片 URL/状态。找不到占位节点则静默跳过。
    let canvas_task_id = task_id.clone();
    let canvas_image_url = image_url.clone();
    let result = with_generation_service_write(app.clone(), move |service, _workspace_service| {
        service.record_output(&task_id, image_url)
    })
    .await?;

    let workspace_dir = app
        .path()
        .app_local_data_dir()
        .map(|dir| dir.join("workspace"))
        .map_err(|e| IpcError::from(AppError::new("resolve workspace dir", e)))?;
    let canvas_write_back = tauri::async_runtime::spawn_blocking(move || {
        if let Some(node_id) =
            crate::application::canvas_memory_rag::complete_generation_task_by_task_id(
                &workspace_dir,
                &canvas_task_id,
                &canvas_image_url,
            )
        {
            eprintln!(
                "[Canvas] async generation written back: task={canvas_task_id} node={node_id}"
            );
        }
    });
    // 写回很快（本地 SQLite），等它完成保证命令返回时画布已更新
    let _ = canvas_write_back.await;

    Ok(result)
}

#[tauri::command]
pub async fn generation_v1_mark_failed(
    app: AppHandle,
    request: MarkFailedRequest,
) -> Result<GenerationTaskRecord, IpcError> {
    with_generation_service_write(app, move |service, _workspace_service| {
        service.mark_failed(&request.task_id, request.error_message)
    })
    .await
}

#[tauri::command]
pub async fn generation_v1_list_tasks(
    app: AppHandle,
    request: ListTasksRequest,
) -> Result<Vec<GenerationTaskRecord>, IpcError> {
    with_generation_service_read(app, move |service, _workspace_service| {
        service.list_tasks(request.status, request.provider_name, request.limit)
    })
    .await
}

#[tauri::command]
pub async fn generation_v1_get_task(
    app: AppHandle,
    request: GetTaskRequest,
) -> Result<Option<GenerationTaskRecord>, IpcError> {
    with_generation_service_read(app, move |service, _workspace_service| {
        service.get_task(&request.id)
    })
    .await
}

#[tauri::command]
pub async fn generation_v1_list_results(
    app: AppHandle,
    request: ListResultsRequest,
) -> Result<Vec<GenerationResultRecord>, IpcError> {
    with_generation_service_read(app, move |service, _workspace_service| {
        service.list_results(&request.task_id)
    })
    .await
}

async fn with_generation_service_read<T, F>(app: AppHandle, operation: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce(&GenerationService, &WorkspaceService) -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let service = app.state::<GenerationService>();
        let workspace_service = app.state::<WorkspaceService>();
        operation(service.inner(), workspace_service.inner())
    })
    .await
    .map_err(|error| {
        eprintln!("generation native task failed: {error}");
        IpcError::task_failed()
    })?
    .map_err(IpcError::from)
}

async fn with_generation_service_write<T, F>(app: AppHandle, operation: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce(&GenerationService, &WorkspaceService) -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let service = app.state::<GenerationService>();
        let workspace_service = app.state::<WorkspaceService>();
        operation(service.inner(), workspace_service.inner())
    })
    .await
    .map_err(|error| {
        eprintln!("generation native task failed: {error}");
        IpcError::task_failed()
    })?
    .map_err(IpcError::from)
}
