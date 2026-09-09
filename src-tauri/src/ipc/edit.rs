//! Edit Understanding Agent IPC 命令处理器。
//!
//! 桥接前端 TypeScript 调用到 Rust EditUnderstandingService。

use tauri::State;

use crate::{
    application::edit_understanding_service::EditUnderstandingService,
    domain::edit::{EditContextType, EditRequestDraft},
    ipc::error::IpcError,
};

/// 提交用户自然语言反馈。
#[tauri::command]
pub fn edit_v1_submit_feedback(
    service: State<'_, EditUnderstandingService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let project_id = str_field(&request, "projectId")?;
    let feedback_text = str_field(&request, "feedbackText")?;
    let context_type = str_field_opt(&request, "contextType").unwrap_or("generation_result");
    let context_ref_id = str_field_opt(&request, "contextRefId");
    let source_review_id = str_field_opt(&request, "sourceReviewId");
    let created_by = str_field_opt(&request, "createdBy").unwrap_or("unknown");

    let draft = EditRequestDraft::try_new(
        project_id.to_owned(),
        None,
        feedback_text.to_owned(),
        context_type.to_owned(),
        context_ref_id.map(|s| s.to_owned()),
        source_review_id.map(|s| s.to_owned()),
        created_by.to_owned(),
    )
    .map_err(|e| IpcError::from(crate::application::error::AppError::EditValidation(e)))?;

    let record = service.submit_feedback(draft)?;
    serde_json::to_value(record).map_err(|e| {
        eprintln!("serialize edit request: {e}");
        IpcError::task_failed()
    })
}

/// 列出项目的所有编辑请求。
#[tauri::command]
pub fn edit_v1_list_requests(
    service: State<'_, EditUnderstandingService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let project_id = str_field(&request, "projectId")?;
    let status = request["status"].as_str().map(|s| {
        crate::domain::edit::EditRequestStatus::parse(s)
            .unwrap_or(crate::domain::edit::EditRequestStatus::Received)
    });
    let limit = request["limit"].as_i64().unwrap_or(50);

    let records = service.list_requests(project_id, status, limit)?;
    serde_json::to_value(records).map_err(|e| {
        eprintln!("serialize edit requests: {e}");
        IpcError::task_failed()
    })
}

/// 获取单条编辑请求。
#[tauri::command]
pub fn edit_v1_get_request(
    service: State<'_, EditUnderstandingService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let id = str_field(&request, "requestId")?;
    let record = service.get_request(id)?;
    serde_json::to_value(record).map_err(|e| {
        eprintln!("serialize edit request: {e}");
        IpcError::task_failed()
    })
}

/// 获取编辑请求对应的编辑计划。
#[tauri::command]
pub fn edit_v1_get_plan_by_request(
    service: State<'_, EditUnderstandingService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let id = str_field(&request, "requestId")?;
    let plan = service.get_plan_by_request(id)?;
    serde_json::to_value(plan).map_err(|e| {
        eprintln!("serialize edit plan: {e}");
        IpcError::task_failed()
    })
}

/// 执行编辑计划。
#[tauri::command]
pub fn edit_v1_apply_plan(
    service: State<'_, EditUnderstandingService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let plan_id = str_field(&request, "planId")?;
    let plan = service.apply_plan(plan_id)?;
    serde_json::to_value(plan).map_err(|e| {
        eprintln!("serialize applied edit plan: {e}");
        IpcError::task_failed()
    })
}

/// 跳过/拒绝一条编辑请求。
#[tauri::command]
pub fn edit_v1_skip_request(
    service: State<'_, EditUnderstandingService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let id = str_field(&request, "requestId")?;
    let reason = str_field_opt(&request, "reason");
    let record = service.skip_request(id, reason)?;
    serde_json::to_value(record).map_err(|e| {
        eprintln!("serialize skipped edit request: {e}");
        IpcError::task_failed()
    })
}

/// 获取编辑请求的 Edit Understanding Agent 解析结果。
#[tauri::command]
pub fn edit_v1_get_understanding_result(
    service: State<'_, EditUnderstandingService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let id = str_field(&request, "requestId")?;
    let record = service.get_request(id)?;
    // 如果 intent_json 不为空，反序列化返回
    if let Some(ref intent_json) = record.and_then(|r| r.intent_json) {
        serde_json::from_str(intent_json).map_err(|e| {
            eprintln!("deserialize edit understanding: {e}");
            IpcError::task_failed()
        })
    } else {
        Ok(serde_json::json!(null))
    }
}

// ─────────────────────────────────────────────────────
// 辅助函数
// ─────────────────────────────────────────────────────

fn str_field<'a>(value: &'a serde_json::Value, field: &'static str) -> Result<&'a str, IpcError> {
    value[field].as_str().ok_or_else(|| IpcError {
        code: "validation_failed",
        message: format!("{}不能为空。", field),
        field: Some(field),
    })
}

fn str_field_opt<'a>(value: &'a serde_json::Value, field: &str) -> Option<&'a str> {
    value[field].as_str()
}
