use tauri::State;

use crate::{
    application::model_router_service::ModelRouterService,
    domain::model_router::{RoutingRequest, RoutingStrategy, RoutingTaskType},
    ipc::error::IpcError,
};

/// 根据任务类型和策略选择最优模型。
#[tauri::command]
pub fn model_router_v1_route(
    service: State<'_, ModelRouterService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let task_type_str = request["taskType"].as_str().ok_or_else(|| IpcError {
        code: "validation_failed",
        message: "taskType 不能为空。".to_owned(),
        field: Some("taskType"),
    })?;

    let task_type = RoutingTaskType::parse(task_type_str).map_err(|e| {
        IpcError::from(crate::application::error::AppError::new(
            "parse task type",
            e,
        ))
    })?;

    let strategy = request["strategy"]
        .as_str()
        .map(|s| match s {
            "cost_optimized" => RoutingStrategy::CostOptimized,
            "quality_first" => RoutingStrategy::QualityFirst,
            "speed_first" => RoutingStrategy::SpeedFirst,
            "balanced" => RoutingStrategy::Balanced,
            _ => RoutingStrategy::Balanced,
        })
        .unwrap_or(RoutingStrategy::Balanced);

    let routing_request = RoutingRequest {
        task_type,
        strategy,
        preferred_provider: request["preferredProvider"].as_str().map(|s| s.to_owned()),
        preferred_model: request["preferredModel"].as_str().map(|s| s.to_owned()),
        budget_limit: request["budgetLimit"].as_f64(),
        requires_reference: request["requiresReference"].as_bool().unwrap_or(false),
        context: request["context"].as_str().map(|s| s.to_owned()),
    };

    let decision = service.route(&routing_request)?;
    serde_json::to_value(decision).map_err(|e| {
        eprintln!("serialize routing decision: {e}");
        IpcError::task_failed()
    })
}

/// 列出可用模型（按任务类型筛选）。
#[tauri::command]
pub fn model_router_v1_list_models(
    service: State<'_, ModelRouterService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let task_type = request["taskType"]
        .as_str()
        .and_then(|s| RoutingTaskType::parse(s).ok());

    let models = service.list_available_models(task_type)?;
    serde_json::to_value(models).map_err(|e| {
        eprintln!("serialize models: {e}");
        IpcError::task_failed()
    })
}

/// 记录模型使用结果（用于运行时学习）。
#[tauri::command]
pub fn model_router_v1_record_outcome(
    service: State<'_, ModelRouterService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let provider_id = request["providerId"].as_str().ok_or_else(|| IpcError {
        code: "validation_failed",
        message: "providerId 不能为空。".to_owned(),
        field: Some("providerId"),
    })?;
    let model_name = request["modelName"].as_str().ok_or_else(|| IpcError {
        code: "validation_failed",
        message: "modelName 不能为空。".to_owned(),
        field: Some("modelName"),
    })?;
    let task_type_str = request["taskType"].as_str().ok_or_else(|| IpcError {
        code: "validation_failed",
        message: "taskType 不能为空。".to_owned(),
        field: Some("taskType"),
    })?;
    let success = request["success"].as_bool().unwrap_or(true);

    let task_type = RoutingTaskType::parse(task_type_str).map_err(|e| {
        IpcError::from(crate::application::error::AppError::new(
            "parse task type",
            e,
        ))
    })?;

    service.record_outcome(provider_id, model_name, task_type, success)?;
    Ok(serde_json::json!({ "recorded": true }))
}
