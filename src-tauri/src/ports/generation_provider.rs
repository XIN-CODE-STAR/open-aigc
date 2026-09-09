#![allow(dead_code)]
//! 生成 Provider 适配器端口。
//!
//! 定义了与 AI Provider 交互的标准接口：提交、轮询、下载。
//! 每个 Provider（Grok、Seedance、Kling 等）实现此 trait。

use crate::domain::providers::ProviderError;

/// 生成请求。
#[derive(Debug, Clone)]
pub struct GenerationSubmitRequest {
    /// Provider 名称。
    pub provider: String,
    /// 模型名称。
    pub model: String,
    /// 生成 Prompt。
    pub prompt: String,
    /// 负面 Prompt（可选）。
    pub negative_prompt: Option<String>,
    /// 额外参数（宽高、时长等）。
    pub parameters: std::collections::HashMap<String, String>,
}

/// 提交结果。
#[derive(Debug, Clone)]
pub struct GenerationSubmitResult {
    /// Provider 返回的远端任务 ID。
    pub remote_job_id: String,
    /// 初始状态。
    pub status: String,
}

/// 轮询结果。
#[derive(Debug, Clone)]
pub struct GenerationPollResult {
    /// 任务状态（pending / running / succeeded / failed）。
    pub status: String,
    /// 进度百分比 0-100。
    pub progress: u8,
    /// 完成时的结果文件 URL。
    pub result_url: Option<String>,
    /// 失败时的错误信息。
    pub error_message: Option<String>,
}

/// 下载结果。
#[derive(Debug, Clone)]
pub struct GenerationDownloadResult {
    /// 下载的文件路径（本地临时路径）。
    pub file_path: String,
    /// 文件 MIME 类型。
    pub mime_type: String,
    /// 文件大小（字节）。
    pub file_size: u64,
}

/// 生成 Provider 适配器端口。
pub trait GenerationProviderAdapter: Send {
    /// 提交生成请求到 Provider。
    fn submit(
        &self,
        request: &GenerationSubmitRequest,
    ) -> Result<GenerationSubmitResult, ProviderError>;

    /// 轮询远端任务状态。
    fn poll(&self, remote_job_id: &str) -> Result<GenerationPollResult, ProviderError>;

    /// 下载生成结果到本地。
    fn download(
        &self,
        result_url: &str,
        target_dir: &str,
    ) -> Result<GenerationDownloadResult, ProviderError>;

    /// 检查 Provider 健康状态。
    fn health_check(&self) -> Result<ProviderHealthStatus, ProviderError>;
}

/// Provider 健康状态。
#[derive(Debug, Clone)]
pub struct ProviderHealthStatus {
    pub available: bool,
    pub message: String,
}
