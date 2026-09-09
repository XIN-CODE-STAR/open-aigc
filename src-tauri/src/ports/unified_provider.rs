//! 统一 Provider 适配器端口。
//!
//! 合并现有 ProviderAdapter 和 GenerationProviderAdapter 为一个统一接口。
//! 所有 Provider（图片、视频、语音）实现此 trait。
//! PollWorker 和 Pipeline 通过 ProviderRegistry 按能力类型查找对应 Provider。

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::domain::credentials::CredentialContext;
use crate::domain::providers::ProviderError;

/// 生成能力类型。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    TextToImage,
    ImageToImage,
    TextToVideo,
    ImageToVideo,
    TextToSpeech,
    VoiceClone,
}

impl CapabilityKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TextToImage => "text_to_image",
            Self::ImageToImage => "image_to_image",
            Self::TextToVideo => "text_to_video",
            Self::ImageToVideo => "image_to_video",
            Self::TextToSpeech => "text_to_speech",
            Self::VoiceClone => "voice_clone",
        }
    }

    /// 是否为高成本操作（需要更严格的成本控制）。
    pub fn is_high_cost(&self) -> bool {
        matches!(self, Self::TextToVideo | Self::ImageToVideo)
    }

    /// 默认轮询间隔（秒）。
    pub fn default_poll_interval_secs(&self) -> u64 {
        match self {
            Self::TextToImage | Self::ImageToImage => 3,
            Self::TextToVideo | Self::ImageToVideo => 8,
            Self::TextToSpeech | Self::VoiceClone => 3,
        }
    }
}

/// 统一生成请求。
#[derive(Debug, Clone)]
pub struct UnifiedRequest {
    /// 能力类型。
    pub capability: CapabilityKind,
    /// 模型名称。
    pub model: String,
    /// 正向提示词。
    pub prompt: String,
    /// 反向提示词。
    pub negative_prompt: Option<String>,
    /// 参考图片本地路径（Image-to-Video / Image-to-Image）。
    pub reference_image_path: Option<String>,
    /// 参考图片远端 URL。
    pub reference_image_url: Option<String>,
    /// 额外参数（width, height, duration, aspect_ratio, seed, cfg_scale 等）。
    pub parameters: HashMap<String, serde_json::Value>,
}

impl UnifiedRequest {
    /// 返回有效模型名。
    /// 当 model 是占位符 "default" 或为空时，回退到 adapter 提供的默认值，
    /// 避免向远端 API 发送无效模型标识。
    pub fn effective_model<'a>(&'a self, fallback: &'a str) -> &'a str {
        if self.model.is_empty() || self.model == "default" {
            fallback
        } else {
            &self.model
        }
    }
}

/// 提交结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedSubmitResult {
    /// 远端任务 ID。
    pub remote_job_id: String,
    /// 初始状态：pending / processing / succeeded（同步 Provider）。
    pub initial_status: String,
    /// 同步 Provider 直接返回的结果 URL（如 Grok 图片）。
    pub immediate_result_url: Option<String>,
    /// 预估完成时间（秒）。
    pub estimated_duration_secs: Option<u64>,
}

/// 轮询结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedPollResult {
    /// pending / processing / succeeded / failed
    pub status: String,
    /// 进度 0-100。
    pub progress: u8,
    /// 完成时的结果 URL。
    pub result_url: Option<String>,
    /// 失败时的错误信息。
    pub error_message: Option<String>,
    /// 是否可重试。
    pub retryable: bool,
}

/// 下载结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedDownloadResult {
    /// 本地文件路径。
    pub file_path: String,
    /// MIME 类型。
    pub mime_type: String,
    /// 文件大小（字节）。
    pub file_size: u64,
    /// 视频/音频时长（秒）。
    pub duration_secs: Option<f64>,
    /// 宽度（像素）。
    pub width: Option<u32>,
    /// 高度（像素）。
    pub height: Option<u32>,
}

/// Provider 健康状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedHealthStatus {
    /// 是否可用。
    pub available: bool,
    /// 诊断信息。
    pub message: String,
    /// 剩余配额（可选）。
    pub quota_remaining: Option<u64>,
}

/// 统一 Provider 适配器端口。所有 Provider 实现此 trait。
///
/// Phase A 变更：submit/poll/download 接受 `&CredentialContext` 参数，
/// adapter 不再内部持有密钥，变为无状态单例。
#[allow(dead_code)]
pub trait UnifiedProviderAdapter: Send + Sync {
    /// Provider 唯一标识（"grok" / "kling" / "seedance"）。
    fn provider_id(&self) -> &str;

    /// 支持的能力列表。
    fn capabilities(&self) -> Vec<CapabilityKind>;

    /// 提交生成请求。
    fn submit(
        &self,
        request: &UnifiedRequest,
        credential: &CredentialContext,
    ) -> Result<UnifiedSubmitResult, ProviderError>;

    /// 轮询远端任务状态。
    fn poll(
        &self,
        remote_job_id: &str,
        credential: &CredentialContext,
    ) -> Result<UnifiedPollResult, ProviderError>;

    /// 下载结果到本地目录。
    fn download(
        &self,
        result_url: &str,
        target_dir: &Path,
        credential: &CredentialContext,
    ) -> Result<UnifiedDownloadResult, ProviderError>;

    /// 健康检查。
    fn health_check(
        &self,
        credential: &CredentialContext,
    ) -> Result<UnifiedHealthStatus, ProviderError>;

    /// 预估单次成本（元）。可选。
    fn estimate_cost(&self, _request: &UnifiedRequest) -> Option<f64> {
        None
    }

    /// 是否异步（同步 Provider 如 Grok 图片返回 false）。
    fn is_async(&self) -> bool {
        true
    }
}
