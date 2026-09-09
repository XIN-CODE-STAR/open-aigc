use std::path::PathBuf;

use crate::application::image_analyzer::{analyze_image, ImageAnalysis};
use crate::ipc::error::IpcError;

/// 本地分析图片文件，提取尺寸、格式、主色调等元信息。
/// 不调用 LLM，纯本地计算，用于为 Agent 提供画布上下文。
#[tauri::command]
pub async fn image_v1_analyze(path: String) -> Result<ImageAnalysis, IpcError> {
    let path_buf = PathBuf::from(&path);
    let analysis = analyze_image(&path_buf)?;
    Ok(analysis)
}
