use tauri::State;

use crate::{application::vision_critic_service::VisionCriticService, ipc::error::IpcError};

/// 使用 LLM Vision API 评价一张图片。
#[tauri::command]
pub fn review_v1_evaluate_with_vision(
    service: State<'_, VisionCriticService>,
    request: serde_json::Value,
) -> Result<serde_json::Value, IpcError> {
    let project_id = request["projectId"]
        .as_str()
        .ok_or_else(IpcError::task_failed)?;
    let asset_id = request["assetId"]
        .as_str()
        .ok_or_else(IpcError::task_failed)?;
    let image_url = request["imageUrl"]
        .as_str()
        .ok_or_else(IpcError::task_failed)?;
    let user_goal = request["userGoal"].as_str();
    let adapter_id = request["adapterId"].as_str();
    let model = request["model"].as_str();
    let api_key = request["apiKey"].as_str();

    let report = service.evaluate_with_vision(
        project_id, asset_id, image_url, user_goal, adapter_id, model, api_key,
    )?;

    serde_json::to_value(report).map_err(|e| {
        eprintln!("serialize vision evaluation: {e}");
        IpcError::task_failed()
    })
}

/// 获取支持的 Vision 适配器列表。
#[tauri::command]
pub fn review_v1_list_vision_adapters(
    _service: State<'_, VisionCriticService>,
) -> Result<serde_json::Value, IpcError> {
    // 返回静态的适配器列表
    let adapters = serde_json::json!([
        {
            "id": "claude-vision",
            "name": "Claude Vision",
            "provider": "Anthropic",
            "models": ["claude-sonnet-4-20250514", "claude-haiku-4-20250514", "claude-3-5-sonnet-20241022"]
        },
        {
            "id": "gpt-vision",
            "name": "GPT Vision",
            "provider": "OpenAI",
            "models": ["gpt-4o", "gpt-4o-mini", "gpt-4-turbo"]
        }
    ]);
    Ok(adapters)
}
