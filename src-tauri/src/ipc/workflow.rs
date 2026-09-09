use tauri::State;

use crate::{
    application::workflow_service::WorkflowService,
    domain::workflow::{WorkflowConfig, WorkflowStage},
    ipc::error::IpcError,
};

/// 启动新的创作工作流。
#[tauri::command]
pub fn workflow_v1_start(
    service: State<'_, WorkflowService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let project_id = str_field(&request, "projectId")?.to_owned();
    let asset_id = str_field(&request, "assetId")?.to_owned();
    let shot_id = request["shotId"].as_str().map(|s| s.to_owned());
    let max_iterations = request["maxIterations"].as_u64().unwrap_or(3) as u32;

    let config = WorkflowConfig {
        max_iterations,
        ..Default::default()
    };

    let state = service.start_workflow(project_id, asset_id, shot_id, config)?;
    serde_json::to_value(state).map_err(|e| {
        eprintln!("serialize workflow: {e}");
        IpcError::task_failed()
    })
}

/// 推进工作流到下一阶段。
#[tauri::command]
pub fn workflow_v1_advance(
    service: State<'_, WorkflowService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let workflow_id = str_field(&request, "workflowId")?.to_owned();
    let stage_str = str_field(&request, "stage")?.to_owned();
    let detail = request["detail"].as_str().map(|s| s.to_owned());

    let stage = WorkflowStage::parse(&stage_str)
        .map_err(|e| IpcError::from(crate::application::error::AppError::new("parse stage", e)))?;

    let state = service.advance_stage(&workflow_id, stage, detail)?;
    serde_json::to_value(state).map_err(|e| {
        eprintln!("serialize workflow: {e}");
        IpcError::task_failed()
    })
}

/// 暂停工作流（等待用户反馈）。
#[tauri::command]
pub fn workflow_v1_pause(
    service: State<'_, WorkflowService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let workflow_id = str_field(&request, "workflowId")?.to_owned();
    let review_id = request["reviewId"].as_str().map(|s| s.to_owned());

    let state = service.pause_for_feedback(&workflow_id, review_id)?;
    serde_json::to_value(state).map_err(|e| {
        eprintln!("serialize workflow: {e}");
        IpcError::task_failed()
    })
}

/// 用户提交反馈后恢复工作流。
#[tauri::command]
pub fn workflow_v1_resume(
    service: State<'_, WorkflowService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let workflow_id = str_field(&request, "workflowId")?.to_owned();
    let edit_request_id = str_field(&request, "editRequestId")?.to_owned();

    let state = service.resume_with_feedback(&workflow_id, edit_request_id)?;
    serde_json::to_value(state).map_err(|e| {
        eprintln!("serialize workflow: {e}");
        IpcError::task_failed()
    })
}

/// 标记修改计划已创建。
#[tauri::command]
pub fn workflow_v1_mark_plan_created(
    service: State<'_, WorkflowService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let workflow_id = str_field(&request, "workflowId")?.to_owned();
    let plan_id = str_field(&request, "planId")?.to_owned();

    let state = service.mark_plan_created(&workflow_id, plan_id)?;
    serde_json::to_value(state).map_err(|e| {
        eprintln!("serialize workflow: {e}");
        IpcError::task_failed()
    })
}

/// 标记工作流失败。
#[tauri::command]
pub fn workflow_v1_fail(
    service: State<'_, WorkflowService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let workflow_id = str_field(&request, "workflowId")?.to_owned();
    let error = request["error"].as_str().unwrap_or("未知错误").to_owned();

    let state = service.mark_failed(&workflow_id, error)?;
    serde_json::to_value(state).map_err(|e| {
        eprintln!("serialize workflow: {e}");
        IpcError::task_failed()
    })
}

/// 获取工作流状态。
#[tauri::command]
pub fn workflow_v1_get(
    service: State<'_, WorkflowService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let workflow_id = str_field(&request, "workflowId")?.to_owned();
    let state = service.get_workflow(&workflow_id)?;
    serde_json::to_value(state).map_err(|e| {
        eprintln!("serialize workflow: {e}");
        IpcError::task_failed()
    })
}

/// 列出项目的工作流。
#[tauri::command]
pub fn workflow_v1_list(
    service: State<'_, WorkflowService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let project_id = str_field(&request, "projectId")?.to_owned();
    let workflows = service.list_workflows(&project_id)?;
    serde_json::to_value(workflows).map_err(|e| {
        eprintln!("serialize workflows: {e}");
        IpcError::task_failed()
    })
}

/// 列出等待反馈的工作流。
#[tauri::command]
pub fn workflow_v1_list_waiting(
    service: State<'_, WorkflowService>,
) -> Result<serde_json::Value, IpcError> {
    let workflows = service.list_waiting_workflows()?;
    serde_json::to_value(workflows).map_err(|e| {
        eprintln!("serialize workflows: {e}");
        IpcError::task_failed()
    })
}

fn str_field<'a>(value: &'a serde_json::Value, field: &'static str) -> Result<&'a str, IpcError> {
    value[field].as_str().ok_or_else(|| IpcError {
        code: "validation_failed",
        message: format!("{}不能为空。", field),
        field: Some(field),
    })
}
