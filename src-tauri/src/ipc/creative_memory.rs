use tauri::State;

use crate::{
    application::creative_memory_service::CreativeMemoryService,
    domain::creative_memory::CreativeMemoryDraft, ipc::error::IpcError,
};

/// 保存新的创意记忆。
#[tauri::command]
pub fn creative_memory_v1_save(
    service: State<'_, CreativeMemoryService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let memory_type = str_field(&request, "memoryType")?;
    let scope = str_field(&request, "scope")?;
    let content_json = str_field(&request, "contentJson")?;
    let summary = str_field(&request, "summary")?;
    let source = str_field(&request, "source")?;
    let created_by = str_field(&request, "createdBy")?;
    let scope_ref_id = str_field_opt(&request, "scopeRefId");
    let source_ref_id = str_field_opt(&request, "sourceRefId");

    let draft = CreativeMemoryDraft::try_new(
        memory_type.to_owned(),
        scope.to_owned(),
        scope_ref_id.map(|s| s.to_owned()),
        content_json.to_owned(),
        summary.to_owned(),
        source.to_owned(),
        source_ref_id.map(|s| s.to_owned()),
        created_by.to_owned(),
    )
    .map_err(|e| IpcError::from(crate::application::error::AppError::from(e)))?;

    let record = service.save_memory(draft)?;
    serde_json::to_value(record).map_err(|e| {
        eprintln!("serialize memory: {e}");
        IpcError::task_failed()
    })
}

/// 获取单条创意记忆。
#[tauri::command]
pub fn creative_memory_v1_get(
    service: State<'_, CreativeMemoryService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let id = str_field(&request, "memoryId")?;
    let record = service.get_memory(id)?;
    serde_json::to_value(record).map_err(|e| {
        eprintln!("serialize memory: {e}");
        IpcError::task_failed()
    })
}

/// 列出创意记忆。
#[tauri::command]
pub fn creative_memory_v1_list(
    service: State<'_, CreativeMemoryService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let memory_type = request["memoryType"].as_str().map(|s| s.to_owned());
    let scope = request["scope"].as_str().map(|s| s.to_owned());
    let scope_ref_id = request["scopeRefId"].as_str().map(|s| s.to_owned());
    let status = request["status"].as_str().map(|s| s.to_owned());
    let limit = request["limit"].as_i64();

    let filter = crate::domain::creative_memory::MemoryFilter::try_new(
        scope,
        scope_ref_id,
        memory_type,
        status,
        limit,
    )
    .map_err(|e| IpcError::from(crate::application::error::AppError::from(e)))?;

    let records = service.list_memories(filter)?;
    serde_json::to_value(records).map_err(|e| {
        eprintln!("serialize memories: {e}");
        IpcError::task_failed()
    })
}

/// 更新创意记忆状态。
#[tauri::command]
pub fn creative_memory_v1_update_status(
    service: State<'_, CreativeMemoryService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let id = str_field(&request, "memoryId")?;
    let status = str_field(&request, "status")?;

    let status = crate::domain::creative_memory::MemoryStatus::parse(status)
        .map_err(|e| IpcError::from(crate::application::error::AppError::from(e)))?;

    let record = service.update_memory_status(id, status)?;
    serde_json::to_value(record).map_err(|e| {
        eprintln!("serialize memory: {e}");
        IpcError::task_failed()
    })
}

/// 更新创意记忆内容。
#[tauri::command]
pub fn creative_memory_v1_update_content(
    service: State<'_, CreativeMemoryService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let id = str_field(&request, "memoryId")?;
    let content_json = str_field(&request, "contentJson")?;
    let summary = str_field(&request, "summary")?;

    let record = service.update_memory_content(id, content_json, summary)?;
    serde_json::to_value(record).map_err(|e| {
        eprintln!("serialize memory: {e}");
        IpcError::task_failed()
    })
}

/// 确认创意记忆（增加置信度）。
#[tauri::command]
pub fn creative_memory_v1_confirm(
    service: State<'_, CreativeMemoryService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let id = str_field(&request, "memoryId")?;
    let record = service.confirm_memory(id)?;
    serde_json::to_value(record).map_err(|e| {
        eprintln!("serialize memory: {e}");
        IpcError::task_failed()
    })
}

/// 删除创意记忆（软删除）。
#[tauri::command]
pub fn creative_memory_v1_delete(
    service: State<'_, CreativeMemoryService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let id = str_field(&request, "memoryId")?;
    service.delete_memory(id)?;
    Ok(serde_json::json!({ "deleted": true }))
}

/// 获取创意记忆的事件历史。
#[tauri::command]
pub fn creative_memory_v1_list_events(
    service: State<'_, CreativeMemoryService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let memory_id = str_field(&request, "memoryId")?;
    let events = service.list_memory_events(memory_id)?;
    serde_json::to_value(events).map_err(|e| {
        eprintln!("serialize events: {e}");
        IpcError::task_failed()
    })
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
