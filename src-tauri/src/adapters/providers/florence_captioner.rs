//! Florence-2 本地 Captioning 适配器。
//!
//! 通过 HTTP 调用本地运行的 Florence-2 Python 服务。
//! Python 服务由 `tools/florence-captioner/caption_server.py` 提供。
//!
//! 架构：
//! Rust (this) --HTTP--> Python (caption_server.py) --> Florence-2 model
//!
//! 适合本地运行，无需 API 费用，但需要 GPU（约 4-6GB VRAM with 8-bit quantization）。

use crate::domain::common::InferenceSource;
use crate::domain::semantic::{SemanticEntity, SemanticTag};
use crate::ports::captioning_port::{CaptionError, CaptionRequest, CaptionResult, CaptioningPort};

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:1433";

/// Florence-2 本地 Captioning 适配器。
pub struct FlorenceCaptioner {
    base_url: String,
    timeout_secs: u64,
}

impl FlorenceCaptioner {
    pub fn new() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_owned(),
            timeout_secs: 120,
        }
    }

    #[allow(dead_code)]
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    #[allow(dead_code)]
    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout_secs = seconds;
        self
    }

    fn health_url(&self) -> String {
        format!("{}/health", self.base_url.trim_end_matches('/'))
    }

    fn caption_url(&self) -> String {
        format!("{}/caption", self.base_url.trim_end_matches('/'))
    }

    /// 从 data URL 中提取 base64 数据部分。
    fn extract_base64(image_url: &str) -> Result<String, CaptionError> {
        if image_url.starts_with("data:") {
            // data:image/png;base64,iVBOR...
            image_url
                .split(',')
                .nth(1)
                .map(String::from)
                .ok_or_else(|| CaptionError::InvalidImage("invalid data URL format".to_owned()))
        } else if image_url.starts_with("http://") || image_url.starts_with("https://") {
            // HTTP URL — 直接传给服务端（服务端需支持 URL 下载，当前版本不支持）
            Err(CaptionError::InvalidImage(
                "HTTP URLs not supported by Florence adapter, use data URLs".to_owned(),
            ))
        } else {
            // 假设已经是 base64
            Ok(image_url.to_owned())
        }
    }
}

impl Default for FlorenceCaptioner {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptioningPort for FlorenceCaptioner {
    fn adapter_id(&self) -> &str {
        "florence-2"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec!["microsoft/Florence-2-base", "microsoft/Florence-2-large"]
    }

    fn is_ready(&self) -> bool {
        // 检查本地服务是否在线
        ureq::get(&self.health_url())
            .call()
            .ok()
            .and_then(|r| r.into_json::<serde_json::Value>().ok())
            .and_then(|v| v.get("loaded").and_then(|l| l.as_bool()))
            .unwrap_or(false)
    }

    fn caption_image(&self, request: &CaptionRequest) -> Result<CaptionResult, CaptionError> {
        let base64_data = Self::extract_base64(&request.image_url)?;

        let body = serde_json::json!({
            "image_base64": base64_data,
        });

        eprintln!(
            "[FlorenceCaptioner] POST {} image_b64_len={}",
            self.caption_url(),
            base64_data.len()
        );

        let response = ureq::post(&self.caption_url())
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|e| CaptionError::ApiError(format!("HTTP request failed: {e}")))?;

        let status = response.status();
        if status >= 400 {
            let body = response
                .into_string()
                .unwrap_or_else(|_| "unknown error".to_owned());
            return Err(CaptionError::ApiError(format!(
                "Florence server returned status {status}: {body}"
            )));
        }

        let result: serde_json::Value = response.into_json().map_err(|e| {
            CaptionError::ParseError(format!("failed to parse Florence response: {e}"))
        })?;

        // 检查错误
        if let Some(error) = result.get("error").and_then(|e| e.as_str()) {
            return Err(CaptionError::ApiError(format!("Florence error: {error}")));
        }

        let caption = result
            .get("caption")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        if caption.is_empty() {
            return Err(CaptionError::ParseError(
                "Florence returned empty caption".to_owned(),
            ));
        }

        let processing_time = result.get("processing_time_ms").and_then(|v| v.as_u64());

        eprintln!(
            "[FlorenceCaptioner] caption_len={} time_ms={:?}",
            caption.len(),
            processing_time
        );

        // 解析 tags
        let tags: Vec<SemanticTag> = result
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        let name = t.get("name")?.as_str()?.to_owned();
                        let confidence = t.get("confidence")?.as_f64()? as f32;
                        Some(SemanticTag {
                            name,
                            confidence: confidence.clamp(0.0, 1.0),
                            source: InferenceSource::Llm,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // 解析 entities
        let entities: Vec<SemanticEntity> = result
            .get("entities")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| {
                        let entity_type = e.get("type")?.as_str()?.to_owned();
                        let name = e.get("name")?.as_str()?.to_owned();
                        let confidence = e.get("confidence")?.as_f64()? as f32;
                        Some(SemanticEntity {
                            entity_type,
                            name,
                            confidence: confidence.clamp(0.0, 1.0),
                            bbox: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // 解析 ocr_text
        let ocr_text = result
            .get("ocr_text")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty() && *s != "null")
            .map(String::from);

        Ok(CaptionResult {
            caption,
            tags,
            entities,
            ocr_text,
            raw_response: serde_json::to_string(&result).unwrap_or_default(),
            tokens_used: None, // 本地模型不消耗 API tokens
        })
    }
}
