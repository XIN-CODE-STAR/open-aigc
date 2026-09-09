#![allow(dead_code)]
//! Grok 生成 Provider 适配器。
//!
//! 通过 Grok API（OpenAI 兼容）实现 GenerationProviderAdapter。
//! 支持文本到图片生成能力。

use crate::domain::providers::ProviderError;
use crate::ports::generation_provider::{
    GenerationDownloadResult, GenerationPollResult, GenerationProviderAdapter,
    GenerationSubmitRequest, GenerationSubmitResult, ProviderHealthStatus,
};

/// Grok 生成适配器配置。
pub struct GrokGenerationConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl Default for GrokGenerationConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.x.ai/v1".to_owned(),
            api_key: String::new(),
            model: "grok-2-image".to_owned(),
        }
    }
}

/// Grok 生成 Provider 适配器。
pub struct GrokGenerationAdapter {
    config: GrokGenerationConfig,
    agent: ureq::Agent,
}

impl GrokGenerationAdapter {
    pub fn new(config: GrokGenerationConfig) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(120))
            .build();
        Self { config, agent }
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.config.api_key)
    }

    fn classify_error(e: ureq::Error) -> ProviderError {
        match e {
            ureq::Error::Status(401, _) => ProviderError::AuthFailed("Invalid API key".into()),
            ureq::Error::Status(429, _) => ProviderError::RateLimited {
                retry_after_seconds: 60,
            },
            ureq::Error::Status(code, resp) => {
                let body = resp.into_string().unwrap_or_default();
                ProviderError::Remote { status: code, body }
            }
            other => ProviderError::Network(other.to_string()),
        }
    }
}

impl GenerationProviderAdapter for GrokGenerationAdapter {
    fn submit(
        &self,
        request: &GenerationSubmitRequest,
    ) -> Result<GenerationSubmitResult, ProviderError> {
        let url = format!("{}/images/generations", self.config.base_url);

        let body = serde_json::json!({
            "model": request.model,
            "prompt": request.prompt,
            "n": 1,
            "response_format": "url"
        });

        let resp: serde_json::Value = self
            .agent
            .post(&url)
            .set("Authorization", &self.auth_header())
            .set("Content-Type", "application/json")
            .send_json(&body)
            .map_err(Self::classify_error)?
            .into_json()
            .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

        let _image_url = resp["data"][0]["url"].as_str().unwrap_or("").to_owned();

        Ok(GenerationSubmitResult {
            remote_job_id: format!("grok-img-{}", uuid::Uuid::new_v4()),
            status: "succeeded".to_owned(),
        })
    }

    fn poll(&self, _remote_job_id: &str) -> Result<GenerationPollResult, ProviderError> {
        Ok(GenerationPollResult {
            status: "succeeded".to_owned(),
            progress: 100,
            result_url: None,
            error_message: None,
        })
    }

    fn download(
        &self,
        result_url: &str,
        target_dir: &str,
    ) -> Result<GenerationDownloadResult, ProviderError> {
        let resp = self
            .agent
            .get(result_url)
            .call()
            .map_err(Self::classify_error)?;

        let content_type = resp
            .header("content-type")
            .unwrap_or("image/png")
            .to_string();

        let file_name = format!("grok-gen-{}.png", uuid::Uuid::new_v4());
        let file_path = std::path::Path::new(target_dir).join(&file_name);

        let mut body = resp.into_reader();
        let mut file =
            std::fs::File::create(&file_path).map_err(|e| ProviderError::Network(e.to_string()))?;

        std::io::copy(&mut body, &mut file).map_err(|e| ProviderError::Network(e.to_string()))?;

        let file_size = std::fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0);

        Ok(GenerationDownloadResult {
            file_path: file_path.to_string_lossy().to_string(),
            mime_type: content_type,
            file_size,
        })
    }

    fn health_check(&self) -> Result<ProviderHealthStatus, ProviderError> {
        let url = format!("{}/models", self.config.base_url);
        match self
            .agent
            .get(&url)
            .set("Authorization", &self.auth_header())
            .call()
        {
            Ok(_) => Ok(ProviderHealthStatus {
                available: true,
                message: "Grok API 可用".to_owned(),
            }),
            Err(ureq::Error::Status(401, _)) => Ok(ProviderHealthStatus {
                available: false,
                message: "API Key 无效或已过期".to_owned(),
            }),
            Err(e) => Ok(ProviderHealthStatus {
                available: false,
                message: format!("连接失败: {e}"),
            }),
        }
    }
}
