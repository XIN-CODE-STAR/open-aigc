use tauri::State;

use crate::{
    application::creative_state_service::CreativeStateService,
    domain::creative_state::{
        CharacterAsset, CreativeStateDraft, DecisionRecord, ReferenceAsset, SceneAsset, StyleTokens,
    },
    ipc::error::IpcError,
};

/// 获取工作区的创作状态。
#[tauri::command]
pub fn creative_state_v1_get(
    service: State<'_, CreativeStateService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let workspace_id = request
        .get("workspaceId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| IpcError {
            code: "validation_failed",
            message: "缺少 workspaceId 参数。".to_owned(),
            field: Some("workspaceId"),
        })?;

    let record = service.get_by_workspace(workspace_id)?;
    serde_json::to_value(record).map_err(|e| {
        eprintln!("serialize creative state: {e}");
        IpcError::task_failed()
    })
}

/// 创建或更新创作状态（upsert）。
#[tauri::command]
pub fn creative_state_v1_upsert(
    service: State<'_, CreativeStateService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let draft: CreativeStateDraft = serde_json::from_value(request).map_err(|e| IpcError {
        code: "validation_failed",
        message: format!("请求参数格式错误：{e}"),
        field: None,
    })?;

    let record = service.upsert(draft)?;
    serde_json::to_value(record).map_err(|e| {
        eprintln!("serialize creative state: {e}");
        IpcError::task_failed()
    })
}

/// 更新风格 Token。
#[tauri::command]
pub fn creative_state_v1_update_style(
    service: State<'_, CreativeStateService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let workspace_id = str_field(&request, "workspaceId")?;
    let tokens: StyleTokens = serde_json::from_value(
        request.get("styleTokens").cloned().unwrap_or_default(),
    )
    .map_err(|e| IpcError {
        code: "validation_failed",
        message: format!("styleTokens 格式错误：{e}"),
        field: Some("styleTokens"),
    })?;

    let record = service.update_style_tokens(workspace_id, tokens)?;
    serde_json::to_value(record).map_err(|e| {
        eprintln!("serialize creative state: {e}");
        IpcError::task_failed()
    })
}

/// 添加角色资产。
#[tauri::command]
pub fn creative_state_v1_add_character(
    service: State<'_, CreativeStateService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let workspace_id = str_field(&request, "workspaceId")?;
    let character: CharacterAsset = serde_json::from_value(
        request.get("character").cloned().unwrap_or_default(),
    )
    .map_err(|e| IpcError {
        code: "validation_failed",
        message: format!("character 格式错误：{e}"),
        field: Some("character"),
    })?;

    let record = service.add_character(workspace_id, character)?;
    serde_json::to_value(record).map_err(|e| {
        eprintln!("serialize creative state: {e}");
        IpcError::task_failed()
    })
}

/// 添加场景资产。
#[tauri::command]
pub fn creative_state_v1_add_scene(
    service: State<'_, CreativeStateService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let workspace_id = str_field(&request, "workspaceId")?;
    let scene: SceneAsset =
        serde_json::from_value(request.get("scene").cloned().unwrap_or_default()).map_err(|e| {
            IpcError {
                code: "validation_failed",
                message: format!("scene 格式错误：{e}"),
                field: Some("scene"),
            }
        })?;

    let record = service.add_scene(workspace_id, scene)?;
    serde_json::to_value(record).map_err(|e| {
        eprintln!("serialize creative state: {e}");
        IpcError::task_failed()
    })
}

/// 添加参考图。
#[tauri::command]
pub fn creative_state_v1_add_reference(
    service: State<'_, CreativeStateService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let workspace_id = str_field(&request, "workspaceId")?;
    let reference: ReferenceAsset = serde_json::from_value(
        request.get("reference").cloned().unwrap_or_default(),
    )
    .map_err(|e| IpcError {
        code: "validation_failed",
        message: format!("reference 格式错误：{e}"),
        field: Some("reference"),
    })?;

    let record = service.add_reference(workspace_id, reference)?;
    serde_json::to_value(record).map_err(|e| {
        eprintln!("serialize creative state: {e}");
        IpcError::task_failed()
    })
}

/// 记录创作决策。
#[tauri::command]
pub fn creative_state_v1_save_decision(
    service: State<'_, CreativeStateService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let workspace_id = str_field(&request, "workspaceId")?;
    let decision: DecisionRecord = serde_json::from_value(
        request.get("decision").cloned().unwrap_or_default(),
    )
    .map_err(|e| IpcError {
        code: "validation_failed",
        message: format!("decision 格式错误：{e}"),
        field: Some("decision"),
    })?;

    let record = service.save_decision(workspace_id, decision)?;
    serde_json::to_value(record).map_err(|e| {
        eprintln!("serialize creative state: {e}");
        IpcError::task_failed()
    })
}

/// 获取风格 Token（快捷接口）。
#[tauri::command]
pub fn creative_state_v1_get_style_tokens(
    service: State<'_, CreativeStateService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let workspace_id = str_field(&request, "workspaceId")?;
    let tokens = service.get_style_tokens(workspace_id)?;
    serde_json::to_value(tokens).map_err(|e| {
        eprintln!("serialize style tokens: {e}");
        IpcError::task_failed()
    })
}

/// 获取 Prompt 上下文（供 Agent 注入）。
#[tauri::command]
pub fn creative_state_v1_get_prompt_context(
    service: State<'_, CreativeStateService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let workspace_id = str_field(&request, "workspaceId")?;
    let context = service.get_prompt_context(workspace_id)?;
    serde_json::to_value(context).map_err(|e| {
        eprintln!("serialize prompt context: {e}");
        IpcError::task_failed()
    })
}

// ── Helper ──

fn str_field<'a>(request: &'a serde_json::Value, field: &str) -> Result<&'a str, IpcError> {
    request
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| IpcError {
            code: "validation_failed",
            message: format!("缺少 {field} 参数。"),
            field: None,
        })
}
