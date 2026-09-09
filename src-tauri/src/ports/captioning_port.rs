//! CaptioningPort — 图片/视频 Captioning 端口。
//!
//! 将视觉资源转化为自然语言描述 + 结构化语义数据。
//! 是 Image Perception Pipeline 的核心端口。
//!
//! 与 VisionAdapter（评分型）不同，CaptioningPort 是描述型：
//! 接收图片，返回 caption、tags、entities、OCR 文本。
//!
//! 设计原则：
//! - 可插拔：DashScope Qwen-VL（云端）、Florence-2（本地）均为实现
//! - 输出直接映射到 ArtifactSemanticProfile
//! - 支持自定义 instruction 以适配不同场景

use std::fmt;

use crate::domain::semantic::{SemanticEntity, SemanticTag};

// ──────────────────────────────────────────────────────────────────
// Errors
// ──────────────────────────────────────────────────────────────────

/// Captioning 错误。
#[derive(Debug)]
pub enum CaptionError {
    /// 网络/API 调用失败。
    ApiError(String),
    /// 响应解析失败。
    ParseError(String),
    /// 图片格式不支持或数据损坏。
    InvalidImage(String),
    /// 适配器未就绪（如本地模型未加载）。
    NotReady(String),
    /// 请求被限流。
    RateLimited { retry_after_ms: Option<u64> },
}

impl fmt::Display for CaptionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ApiError(msg) => write!(f, "captioning API error: {msg}"),
            Self::ParseError(msg) => write!(f, "captioning parse error: {msg}"),
            Self::InvalidImage(msg) => write!(f, "invalid image: {msg}"),
            Self::NotReady(msg) => write!(f, "captioner not ready: {msg}"),
            Self::RateLimited { retry_after_ms } => {
                write!(f, "rate limited")?;
                if let Some(ms) = retry_after_ms {
                    write!(f, ", retry after {ms}ms")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for CaptionError {}

// ──────────────────────────────────────────────────────────────────
// Request / Response
// ──────────────────────────────────────────────────────────────────

/// Captioning 请求。
#[derive(Debug, Clone)]
pub struct CaptionRequest {
    /// 图片 data URL（"data:image/...;base64,..."）或 HTTP URL。
    pub image_url: String,
    /// 自定义分析指令（覆盖默认 prompt）。
    pub instruction: Option<String>,
    /// 关联的资源 ID（用于结果追溯）。
    pub asset_id: Option<String>,
}

/// Captioning 结果。
#[derive(Debug, Clone)]
pub struct CaptionResult {
    /// 自然语言描述（2-4 句话）。
    pub caption: String,
    /// 语义标签（名称 + 置信度）。
    pub tags: Vec<SemanticTag>,
    /// 识别的实体。
    pub entities: Vec<SemanticEntity>,
    /// OCR 提取的文本（如有）。
    pub ocr_text: Option<String>,
    /// 原始 API 响应（用于调试）。
    pub raw_response: String,
    /// 消耗的 token 数量。
    pub tokens_used: Option<u32>,
}

/// 适配器信息。
#[derive(Debug, Clone)]
pub struct CaptionerInfo {
    /// 适配器标识（如 "dashscope-qwen-vl" / "florence-2"）。
    pub adapter_id: String,
    /// 支持的模型列表。
    pub supported_models: Vec<String>,
    /// 是否就绪（本地模型需检查加载状态）。
    pub ready: bool,
}

// ──────────────────────────────────────────────────────────────────
// CaptioningPort trait
// ──────────────────────────────────────────────────────────────────

/// 图片/视频 Captioning 端口。
///
/// 实现者：
/// - `DashScopeCaptioner`：调用阿里云 DashScope Qwen-VL API（云端）
/// - `FlorenceCaptioner`：调用本地 Florence-2 Python 服务（本地）
///
/// 输出直接映射到 `ArtifactSemanticProfile`，由 `SemanticPipelineService` 消费。
pub trait CaptioningPort: Send + Sync {
    /// 适配器标识。
    fn adapter_id(&self) -> &str;

    /// 支持的模型列表。
    fn supported_models(&self) -> Vec<&str>;

    /// 适配器是否就绪。
    fn is_ready(&self) -> bool;

    /// 适配器详细信息。
    fn info(&self) -> CaptionerInfo {
        CaptionerInfo {
            adapter_id: self.adapter_id().to_owned(),
            supported_models: self
                .supported_models()
                .into_iter()
                .map(String::from)
                .collect(),
            ready: self.is_ready(),
        }
    }

    /// 对单张图片进行 captioning。
    fn caption_image(&self, request: &CaptionRequest) -> Result<CaptionResult, CaptionError>;

    /// 批量 captioning（默认实现：逐张调用）。
    fn caption_batch(
        &self,
        requests: &[CaptionRequest],
    ) -> Result<Vec<CaptionResult>, CaptionError> {
        let mut results = Vec::with_capacity(requests.len());
        for request in requests {
            results.push(self.caption_image(request)?);
        }
        Ok(results)
    }
}
