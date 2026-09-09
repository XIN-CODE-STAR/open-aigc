//! Grok UnifiedProviderAdapter 实现。
//!
//! 将 Grok 图片生成适配为统一 Provider 接口。
//! Grok 图片是同步 Provider：submit 直接返回结果 URL，不需要轮询。
//!
//! Phase A 变更：adapter 不再内部持有 api_key，改为从 CredentialContext 参数获取。

use std::path::Path;
use std::time::Duration;

use crate::domain::credentials::CredentialContext;
use crate::domain::providers::ProviderError;
use crate::ports::unified_provider::{
    CapabilityKind, UnifiedDownloadResult, UnifiedHealthStatus, UnifiedPollResult,
    UnifiedProviderAdapter, UnifiedRequest, UnifiedSubmitResult,
};

/// Grok Unified 适配器配置（仅保留非密钥默认值）。
#[derive(Debug, Clone)]
pub struct GrokUnifiedConfig {
    pub base_url: String,
    pub model: String,
}

impl Default for GrokUnifiedConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.x.ai/v1".into(),
            model: "grok-2-image".into(),
        }
    }
}

/// Grok Unified 适配器（同步图片生成）。无状态单例，密钥从 CredentialContext 获取。
pub struct GrokUnifiedAdapter {
    config: GrokUnifiedConfig,
    agent: ureq::Agent,
}

impl GrokUnifiedAdapter {
    pub fn new(config: GrokUnifiedConfig) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(120))
            .build();
        Self { config, agent }
    }

    /// 从 CredentialContext 提取 API Key。
    fn api_key(credential: &CredentialContext) -> Result<&str, ProviderError> {
        credential
            .api_key()
            .ok_or_else(|| ProviderError::ConfigInvalid("missing api_key in credential".into()))
    }

    /// 有效 base_url：credential 优先，fallback 到 config 默认值。
    fn effective_base_url<'a>(&'a self, credential: &'a CredentialContext) -> &'a str {
        if credential.base_url.is_empty() {
            &self.config.base_url
        } else {
            &credential.base_url
        }
    }

    /// 有效 model：credential 优先，fallback 到 config 默认值。
    fn effective_model<'a>(&'a self, credential: &'a CredentialContext) -> &'a str {
        if credential.model.is_empty() {
            &self.config.model
        } else {
            &credential.model
        }
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

impl UnifiedProviderAdapter for GrokUnifiedAdapter {
    fn provider_id(&self) -> &str {
        "grok"
    }

    fn capabilities(&self) -> Vec<CapabilityKind> {
        vec![CapabilityKind::TextToImage]
    }

    fn submit(
        &self,
        request: &UnifiedRequest,
        credential: &CredentialContext,
    ) -> Result<UnifiedSubmitResult, ProviderError> {
        let api_key = Self::api_key(credential)?;
        let base_url = self.effective_base_url(credential);
        let model = request.effective_model(self.effective_model(credential));
        let url = format!("{base_url}/images/generations");

        let body = serde_json::json!({
            "model": model,
            "prompt": request.prompt,
            "n": 1,
            "response_format": "url"
        });

        let resp: serde_json::Value = self
            .agent
            .post(&url)
            .set("Authorization", &format!("Bearer {api_key}"))
            .set("Content-Type", "application/json")
            .send_json(&body)
            .map_err(Self::classify_error)?
            .into_json()
            .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

        let image_url = resp["data"][0]["url"].as_str().unwrap_or("").to_owned();

        Ok(UnifiedSubmitResult {
            remote_job_id: format!("grok-img-{}", uuid::Uuid::new_v4()),
            initial_status: "succeeded".into(),
            immediate_result_url: Some(image_url),
            estimated_duration_secs: Some(0),
        })
    }

    fn poll(
        &self,
        _remote_job_id: &str,
        _credential: &CredentialContext,
    ) -> Result<UnifiedPollResult, ProviderError> {
        // 同步 Provider，poll 永远返回 succeeded
        Ok(UnifiedPollResult {
            status: "succeeded".into(),
            progress: 100,
            result_url: None,
            error_message: None,
            retryable: false,
        })
    }

    fn download(
        &self,
        result_url: &str,
        target_dir: &Path,
        _credential: &CredentialContext,
    ) -> Result<UnifiedDownloadResult, ProviderError> {
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
        let file_path = target_dir.join(&file_name);

        let mut body = resp.into_reader();
        let mut file =
            std::fs::File::create(&file_path).map_err(|e| ProviderError::Network(e.to_string()))?;

        std::io::copy(&mut body, &mut file).map_err(|e| ProviderError::Network(e.to_string()))?;

        let file_size = std::fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0);

        Ok(UnifiedDownloadResult {
            file_path: file_path.to_string_lossy().to_string(),
            mime_type: content_type,
            file_size,
            duration_secs: None,
            width: None,
            height: None,
        })
    }

    fn health_check(
        &self,
        credential: &CredentialContext,
    ) -> Result<UnifiedHealthStatus, ProviderError> {
        let api_key = Self::api_key(credential)?;
        let base_url = self.effective_base_url(credential);
        let url = format!("{base_url}/models");
        match self
            .agent
            .get(&url)
            .set("Authorization", &format!("Bearer {api_key}"))
            .call()
        {
            Ok(_) => Ok(UnifiedHealthStatus {
                available: true,
                message: "Grok API 可用".into(),
                quota_remaining: None,
            }),
            Err(ureq::Error::Status(401, _)) => Ok(UnifiedHealthStatus {
                available: false,
                message: "API Key 无效或已过期".into(),
                quota_remaining: None,
            }),
            Err(e) => Ok(UnifiedHealthStatus {
                available: false,
                message: format!("连接失败: {e}"),
                quota_remaining: None,
            }),
        }
    }

    fn is_async(&self) -> bool {
        false // 同步 Provider
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::credentials::CredentialType;

    fn test_credential() -> CredentialContext {
        CredentialContext {
            provider_id: "grok".to_owned(),
            credential_type: CredentialType::ApiKey,
            payload: serde_json::json!({ "api_key": "sk-test" }),
            base_url: String::new(),
            model: String::new(),
        }
    }

    #[test]
    fn grok_unified_is_sync() {
        let adapter = GrokUnifiedAdapter::new(GrokUnifiedConfig::default());
        assert!(!adapter.is_async());
        assert_eq!(adapter.provider_id(), "grok");
        assert!(adapter
            .capabilities()
            .contains(&CapabilityKind::TextToImage));
    }

    #[test]
    fn grok_unified_poll_always_succeeds() {
        let adapter = GrokUnifiedAdapter::new(GrokUnifiedConfig::default());
        let result = adapter.poll("any-id", &test_credential()).unwrap();
        assert_eq!(result.status, "succeeded");
        assert_eq!(result.progress, 100);
    }
}
