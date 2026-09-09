//! AI Critic Agent IPC 命令处理器（完整版）。
//!
//! 桥接前端 TypeScript 调用到 Rust CriticService。

use tauri::State;

use crate::{application::critic_service::CriticService, ipc::error::IpcError};

/// 列出项目的所有评价报告。
#[tauri::command]
pub fn review_v1_list_reports(
    service: State<'_, CriticService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let project_id = request["projectId"]
        .as_str()
        .ok_or_else(IpcError::task_failed)?;
    let decision = request["decision"].as_str().map(|d| {
        crate::domain::review::ReviewDecision::parse(d)
            .unwrap_or(crate::domain::review::ReviewDecision::NeedsReview)
    });
    let limit = request["limit"].as_i64().unwrap_or(50);

    let reports = service.list_reports_by_project(project_id, decision, limit)?;
    serde_json::to_value(reports).map_err(|e| {
        eprintln!("serialize review reports: {e}");
        IpcError::task_failed()
    })
}

/// 获取单条评价报告。
#[tauri::command]
pub fn review_v1_get_report(
    service: State<'_, CriticService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let id = request["reportId"]
        .as_str()
        .ok_or_else(IpcError::task_failed)?;
    let report = service.get_report(id)?;
    serde_json::to_value(report).map_err(|e| {
        eprintln!("serialize review report: {e}");
        IpcError::task_failed()
    })
}

/// 列出某条评价报告的维度详情。
#[tauri::command]
pub fn review_v1_list_dimensions(
    service: State<'_, CriticService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let review_id = request["reviewId"]
        .as_str()
        .ok_or_else(IpcError::task_failed)?;
    let dimensions = service.list_dimensions(review_id)?;
    serde_json::to_value(dimensions).map_err(|e| {
        eprintln!("serialize dimensions: {e}");
        IpcError::task_failed()
    })
}

/// 列出资产的版本历史。
#[tauri::command]
pub fn asset_v1_list_versions(
    service: State<'_, CriticService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let asset_id = request["assetId"]
        .as_str()
        .ok_or_else(IpcError::task_failed)?;
    let limit = request["limit"].as_i64().unwrap_or(50);
    let versions = service.list_asset_versions(asset_id, limit)?;
    serde_json::to_value(versions).map_err(|e| {
        eprintln!("serialize asset versions: {e}");
        IpcError::task_failed()
    })
}

/// 获取资产的版权信息。
#[tauri::command]
pub fn asset_v1_get_license(
    service: State<'_, CriticService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let asset_id = request["assetId"]
        .as_str()
        .ok_or_else(IpcError::task_failed)?;
    let license = service.get_asset_license(asset_id)?;
    serde_json::to_value(license).map_err(|e| {
        eprintln!("serialize asset license: {e}");
        IpcError::task_failed()
    })
}

/// 获取内容安全检查报告（如果没有则运行规则引擎并返回）。
#[tauri::command]
pub fn content_guard_v1_get_report(
    service: State<'_, CriticService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let target_type = request["targetType"]
        .as_str()
        .ok_or_else(IpcError::task_failed)?;
    let target_id = request["targetId"].as_str();

    // 优先查已有报告
    if let Some(tid) = target_id {
        if let Ok(Some(report)) = service.get_latest_guard_report(target_type, tid) {
            return serde_json::to_value(report).map_err(|e| {
                eprintln!("serialize guard report: {e}");
                IpcError::task_failed()
            });
        }
    }

    // 没有已有报告 → 运行规则引擎
    // 注意：content_guard 需要 project_id。如果请求包含 project_id，使用之；
    // 否则使用 target_id 作为 project_id 的回退。
    let project_id = request["projectId"]
        .as_str()
        .or(target_id)
        .unwrap_or("unknown");

    // 提取内容文本用于规则检查
    let content_text = request["contentText"].as_str();

    let report = service.run_content_guard_check(
        project_id.to_owned(),
        target_type,
        target_id,
        content_text,
    )?;

    serde_json::to_value(report).map_err(|e| {
        eprintln!("serialize guard report: {e}");
        IpcError::task_failed()
    })
}
