//! Kling 视频 Provider 适配器。
//!
//! 接入 Kling（快影）视频生成 API，支持：
//! - Text-to-Video（文生视频）
//! - Image-to-Video（图生视频）
//! - Text-to-Image（文生图）
//!
//! 认证方式：AK/SK 签发 JWT（HS256），有效期 30 分钟。
//! API 基础地址：https://api.klingai.com
//!
//! Phase A 重构：Adapter 无状态，不持有密钥。
//! 每次调用通过 &CredentialContext 获取 access_key / secret_key。

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::domain::credentials::CredentialContext;
use crate::domain::providers::ProviderError;
use crate::ports::unified_provider::{
    CapabilityKind, UnifiedDownloadResult, UnifiedHealthStatus, UnifiedPollResult,
    UnifiedProviderAdapter, UnifiedRequest, UnifiedSubmitResult,
};

/// Kling 视频适配器配置（仅含非敏感默认值）。
#[derive(Debug, Clone)]
pub struct KlingConfig {
    pub base_url: String,
    /// 默认模型名。当请求和 credential 均未指定有效 model 时回退到此值。
    pub default_model: String,
}

impl Default for KlingConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.klingai.com".into(),
            default_model: "kling-v1".into(),
        }
    }
}

/// Kling 视频 Provider 适配器（无状态，不持有密钥）。
pub struct KlingVideoAdapter {
    config: KlingConfig,
    agent: ureq::Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KlingTaskKind {
    TextToVideo,
    ImageToVideo,
    TextToImage,
}

impl KlingTaskKind {
    fn from_capability(capability: &CapabilityKind) -> Option<Self> {
        match capability {
            CapabilityKind::TextToVideo => Some(Self::TextToVideo),
            CapabilityKind::ImageToVideo => Some(Self::ImageToVideo),
            CapabilityKind::TextToImage => Some(Self::TextToImage),
            _ => None,
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Self::TextToVideo => "text2video",
            Self::ImageToVideo => "image2video",
            Self::TextToImage => "text2image",
        }
    }

    fn poll_path(self, remote_task_id: &str) -> String {
        match self {
            Self::TextToVideo => format!("/v1/videos/text2video/{remote_task_id}"),
            Self::ImageToVideo => format!("/v1/videos/image2video/{remote_task_id}"),
            Self::TextToImage => format!("/v1/images/generations/{remote_task_id}"),
        }
    }
}

fn encode_remote_job_id(kind: KlingTaskKind, remote_task_id: &str) -> String {
    format!("{}:{remote_task_id}", kind.prefix())
}

fn decode_remote_job_id(remote_job_id: &str) -> (KlingTaskKind, &str) {
    if let Some((prefix, raw_id)) = remote_job_id.split_once(':') {
        let kind = match prefix {
            "text2video" => KlingTaskKind::TextToVideo,
            "image2video" => KlingTaskKind::ImageToVideo,
            "text2image" => KlingTaskKind::TextToImage,
            _ => KlingTaskKind::ImageToVideo,
        };
        (kind, raw_id)
    } else {
        (KlingTaskKind::ImageToVideo, remote_job_id)
    }
}

impl KlingVideoAdapter {
    pub fn new(config: KlingConfig) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(300))
            .build();
        Self { config, agent }
    }

    /// 从 CredentialContext 提取 access_key。
    fn access_key(credential: &CredentialContext) -> Result<String, ProviderError> {
        credential
            .access_key()
            .map(|s| s.to_owned())
            .ok_or_else(|| ProviderError::ConfigInvalid("Kling credential 缺少 access_key".into()))
    }

    /// 从 CredentialContext 提取 secret_key。
    fn secret_key(credential: &CredentialContext) -> Result<String, ProviderError> {
        credential
            .secret_key()
            .map(|s| s.to_owned())
            .ok_or_else(|| ProviderError::ConfigInvalid("Kling credential 缺少 secret_key".into()))
    }

    /// 有效 base_url：credential 优先，回退到 config 默认值。
    fn effective_base_url(&self, credential: &CredentialContext) -> String {
        if credential.base_url.is_empty() {
            self.config.base_url.clone()
        } else {
            credential.base_url.clone()
        }
    }

    /// 有效模型名：request > credential > config 默认值。
    fn effective_model<'a>(
        &'a self,
        request: &'a UnifiedRequest,
        credential: &'a CredentialContext,
    ) -> &'a str {
        if !request.model.is_empty() && request.model != "default" {
            &request.model
        } else if !credential.model.is_empty() {
            &credential.model
        } else {
            &self.config.default_model
        }
    }

    /// 签发 JWT Token（HS256），使用 credential 中的 AK/SK。
    fn generate_jwt(&self, credential: &CredentialContext) -> Result<String, ProviderError> {
        let access_key = Self::access_key(credential)?;
        let secret_key = Self::secret_key(credential)?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // JWT: header.payload.signature
        // payload: { "iss": access_key, "exp": now+1800, "nbf": now-5 }
        let header = base64_encode(r#"{"alg":"HS256","typ":"JWT"}"#);
        let payload = base64_encode(&format!(
            r#"{{"iss":"{}","exp":{},"nbf":{}}}"#,
            access_key,
            now + 1800,
            now.saturating_sub(5)
        ));
        let signature_input = format!("{header}.{payload}");
        let signature = hmac_sha256(&signature_input, &secret_key);
        Ok(format!("{header}.{payload}.{signature}"))
    }

    /// 构建请求体。
    fn build_request_body(
        &self,
        request: &UnifiedRequest,
        credential: &CredentialContext,
    ) -> Result<(String, serde_json::Value), ProviderError> {
        let base_url = self.effective_base_url(credential);
        let model = self.effective_model(request, credential);

        match request.capability {
            CapabilityKind::ImageToVideo => {
                let url = format!("{base_url}/v1/videos/image2video");
                let body = serde_json::json!({
                    "model_name": model,
                    "image": request.reference_image_url,
                    "prompt": request.prompt,
                    "negative_prompt": request.negative_prompt,
                    "cfg_scale": request.parameters.get("cfg_scale").and_then(|v| v.as_f64()).unwrap_or(0.5),
                    "duration": request.parameters.get("duration").and_then(|v| v.as_str()).unwrap_or("5"),
                    "mode": request.parameters.get("mode").and_then(|v| v.as_str()).unwrap_or("std"),
                });
                Ok((url, body))
            }
            CapabilityKind::TextToVideo => {
                let url = format!("{base_url}/v1/videos/text2video");
                let body = serde_json::json!({
                    "model_name": model,
                    "prompt": request.prompt,
                    "negative_prompt": request.negative_prompt,
                    "duration": request.parameters.get("duration").and_then(|v| v.as_str()).unwrap_or("5"),
                    "aspect_ratio": request.parameters.get("aspect_ratio").and_then(|v| v.as_str()).unwrap_or("16:9"),
                });
                Ok((url, body))
            }
            CapabilityKind::TextToImage => {
                let url = format!("{base_url}/v1/images/generations");
                let body = serde_json::json!({
                    "model_name": model,
                    "prompt": request.prompt,
                    "negative_prompt": request.negative_prompt,
                    "n": 1,
                    "size": request.parameters.get("size").and_then(|v| v.as_str()).unwrap_or("1024x1024"),
                });
                Ok((url, body))
            }
            _ => Err(ProviderError::ConfigInvalid(format!(
                "Kling 不支持的能力：{:?}",
                request.capability
            ))),
        }
    }
}

impl UnifiedProviderAdapter for KlingVideoAdapter {
    fn provider_id(&self) -> &str {
        "kling"
    }

    fn capabilities(&self) -> Vec<CapabilityKind> {
        vec![
            CapabilityKind::TextToVideo,
            CapabilityKind::ImageToVideo,
            CapabilityKind::TextToImage,
        ]
    }

    fn submit(
        &self,
        request: &UnifiedRequest,
        credential: &CredentialContext,
    ) -> Result<UnifiedSubmitResult, ProviderError> {
        let token = self.generate_jwt(credential)?;
        let (url, body) = self.build_request_body(request, credential)?;
        let task_kind = KlingTaskKind::from_capability(&request.capability).ok_or_else(|| {
            ProviderError::ConfigInvalid(format!(
                "Kling unsupported capability: {:?}",
                request.capability
            ))
        })?;

        let resp: serde_json::Value = self
            .agent
            .post(&url)
            .set("Authorization", &format!("Bearer {token}"))
            .set("Content-Type", "application/json")
            .send_json(&body)
            .map_err(classify_kling_error)?
            .into_json()
            .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

        if resp["code"].as_i64().unwrap_or(-1) != 0 {
            return Err(ProviderError::Remote {
                status: resp["code"].as_u64().unwrap_or(0) as u16,
                body: resp["message"].as_str().unwrap_or("unknown").to_owned(),
            });
        }

        Ok(UnifiedSubmitResult {
            remote_job_id: encode_remote_job_id(
                task_kind,
                resp["data"]["task_id"].as_str().unwrap_or(""),
            ),
            initial_status: resp["data"]["task_status"]
                .as_str()
                .unwrap_or("submitted")
                .to_owned(),
            immediate_result_url: None,
            estimated_duration_secs: Some(120),
        })
    }

    fn poll(
        &self,
        remote_job_id: &str,
        credential: &CredentialContext,
    ) -> Result<UnifiedPollResult, ProviderError> {
        let token = self.generate_jwt(credential)?;
        let (task_kind, raw_remote_job_id) = decode_remote_job_id(remote_job_id);
        let base_url = self.effective_base_url(credential);
        let url = format!(
            "{}{}",
            base_url.trim_end_matches('/'),
            task_kind.poll_path(raw_remote_job_id)
        );

        let resp: serde_json::Value = self
            .agent
            .get(&url)
            .set("Authorization", &format!("Bearer {token}"))
            .call()
            .map_err(classify_kling_error)?
            .into_json()
            .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

        let task_status = resp["data"]["task_status"].as_str().unwrap_or("processing");
        match task_status {
            "succeed" => {
                let result_url = match task_kind {
                    KlingTaskKind::TextToImage => resp["data"]["task_result"]["images"][0]["url"]
                        .as_str()
                        .map(|s| s.to_owned()),
                    KlingTaskKind::TextToVideo | KlingTaskKind::ImageToVideo => resp["data"]
                        ["task_result"]["videos"][0]["url"]
                        .as_str()
                        .map(|s| s.to_owned()),
                };
                Ok(UnifiedPollResult {
                    status: "succeeded".to_owned(),
                    progress: 100,
                    result_url,
                    error_message: None,
                    retryable: false,
                })
            }
            "failed" => Ok(UnifiedPollResult {
                status: "failed".to_owned(),
                progress: 0,
                result_url: None,
                error_message: Some(
                    resp["data"]["task_status_msg"]
                        .as_str()
                        .unwrap_or("failed")
                        .to_owned(),
                ),
                retryable: true,
            }),
            _ => Ok(UnifiedPollResult {
                status: "processing".to_owned(),
                progress: 50,
                result_url: None,
                error_message: None,
                retryable: false,
            }),
        }
    }

    fn download(
        &self,
        result_url: &str,
        target_dir: &Path,
        _credential: &CredentialContext,
    ) -> Result<UnifiedDownloadResult, ProviderError> {
        // 下载结果文件不需要认证（URL 自带签名）
        let resp = self
            .agent
            .get(result_url)
            .call()
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let file_name = format!("kling-{}.mp4", uuid::Uuid::new_v4());
        let file_path = target_dir.join(&file_name);
        let mut body = resp.into_reader();
        let mut file =
            std::fs::File::create(&file_path).map_err(|e| ProviderError::Network(e.to_string()))?;
        std::io::copy(&mut body, &mut file).map_err(|e| ProviderError::Network(e.to_string()))?;
        let file_size = std::fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0);
        Ok(UnifiedDownloadResult {
            file_path: file_path.to_string_lossy().to_string(),
            mime_type: "video/mp4".to_owned(),
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
        let token = self.generate_jwt(credential)?;
        let base_url = self.effective_base_url(credential);
        match self
            .agent
            .get(&format!("{base_url}/v1/models"))
            .set("Authorization", &format!("Bearer {token}"))
            .call()
        {
            Ok(_) => Ok(UnifiedHealthStatus {
                available: true,
                message: "Kling API 可用".to_owned(),
                quota_remaining: None,
            }),
            Err(e) => Ok(UnifiedHealthStatus {
                available: false,
                message: format!("连接失败: {e}"),
                quota_remaining: None,
            }),
        }
    }

    fn estimate_cost(&self, request: &UnifiedRequest) -> Option<f64> {
        match request.capability {
            CapabilityKind::TextToVideo | CapabilityKind::ImageToVideo => {
                let mode = request
                    .parameters
                    .get("mode")
                    .and_then(|v| v.as_str())
                    .unwrap_or("std");
                let duration = request
                    .parameters
                    .get("duration")
                    .and_then(|v| v.as_str())
                    .unwrap_or("5");
                let base = if mode == "pro" { 0.7 } else { 0.3 };
                let factor = duration.parse::<f64>().unwrap_or(5.0) / 5.0;
                Some(base * factor)
            }
            _ => Some(0.05),
        }
    }

    fn is_async(&self) -> bool {
        true
    }
}

// ─────────────────────────────────────────────────────
// 工具函数
// ─────────────────────────────────────────────────────

fn classify_kling_error(error: ureq::Error) -> ProviderError {
    ProviderError::Network(error.to_string())
}

fn base64_encode(input: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(input.as_bytes())
}

fn hmac_sha256(input: &str, key: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("HMAC accepts any key length");
    mac.update(input.as_bytes());
    let result = mac.finalize();
    hex_encode(&result.into_bytes())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_credential() -> CredentialContext {
        CredentialContext {
            provider_id: "kling".into(),
            credential_type: crate::domain::credentials::CredentialType::AccessSecret,
            payload: serde_json::json!({
                "access_key": "test-ak",
                "secret_key": "test-sk"
            }),
            base_url: String::new(),
            model: String::new(),
        }
    }

    #[test]
    fn kling_adapter_capabilities() {
        let adapter = KlingVideoAdapter::new(KlingConfig::default());
        assert_eq!(adapter.provider_id(), "kling");
        let caps = adapter.capabilities();
        assert!(caps.contains(&CapabilityKind::TextToVideo));
        assert!(caps.contains(&CapabilityKind::ImageToVideo));
        assert!(caps.contains(&CapabilityKind::TextToImage));
        assert!(!caps.contains(&CapabilityKind::TextToSpeech));
    }

    #[test]
    fn kling_is_async() {
        let adapter = KlingVideoAdapter::new(KlingConfig::default());
        assert!(adapter.is_async());
    }

    #[test]
    fn kling_jwt_generation_uses_credential() {
        let adapter = KlingVideoAdapter::new(KlingConfig::default());
        let credential = test_credential();
        let jwt = adapter.generate_jwt(&credential).unwrap();
        // JWT 应该是三段 base64
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        // payload 应包含 access_key
        let payload_decoded = {
            use base64::Engine;
            let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(parts[1])
                .unwrap();
            String::from_utf8(bytes).unwrap()
        };
        assert!(payload_decoded.contains("test-ak"));
    }

    #[test]
    fn kling_jwt_fails_without_keys() {
        let adapter = KlingVideoAdapter::new(KlingConfig::default());
        let empty_credential = CredentialContext {
            provider_id: "kling".into(),
            credential_type: crate::domain::credentials::CredentialType::AccessSecret,
            payload: serde_json::json!({}),
            base_url: String::new(),
            model: String::new(),
        };
        let result = adapter.generate_jwt(&empty_credential);
        assert!(result.is_err());
    }

    #[test]
    fn kling_cost_estimation() {
        let adapter = KlingVideoAdapter::new(KlingConfig::default());
        let request = UnifiedRequest {
            capability: CapabilityKind::TextToVideo,
            model: "kling-v1".into(),
            prompt: "test".into(),
            negative_prompt: None,
            reference_image_path: None,
            reference_image_url: None,
            parameters: {
                let mut m = std::collections::HashMap::new();
                m.insert("duration".into(), serde_json::json!("5"));
                m.insert("mode".into(), serde_json::json!("std"));
                m
            },
        };
        let cost = adapter.estimate_cost(&request);
        assert!(cost.is_some());
        assert!((cost.unwrap() - 0.3).abs() < 0.01);
    }

    #[test]
    fn hex_encode_works() {
        assert_eq!(hex_encode(&[0x48, 0x65, 0x6c]), "48656c");
    }

    #[test]
    fn remote_job_id_preserves_task_kind_for_polling() {
        let encoded = encode_remote_job_id(KlingTaskKind::TextToVideo, "remote-123");
        assert_eq!(encoded, "text2video:remote-123");

        let (kind, raw_id) = decode_remote_job_id(&encoded);
        assert_eq!(kind, KlingTaskKind::TextToVideo);
        assert_eq!(raw_id, "remote-123");
        assert_eq!(kind.poll_path(raw_id), "/v1/videos/text2video/remote-123");

        let (legacy_kind, legacy_raw_id) = decode_remote_job_id("legacy-456");
        assert_eq!(legacy_kind, KlingTaskKind::ImageToVideo);
        assert_eq!(legacy_raw_id, "legacy-456");
    }

    #[test]
    fn effective_model_priority_chain() {
        let adapter = KlingVideoAdapter::new(KlingConfig::default());
        let credential = CredentialContext {
            provider_id: "kling".into(),
            credential_type: crate::domain::credentials::CredentialType::AccessSecret,
            payload: serde_json::json!({"access_key":"ak","secret_key":"sk"}),
            base_url: String::new(),
            model: "kling-v2-credential".into(),
        };

        // request 有明确 model → 用 request 的
        let req = UnifiedRequest {
            capability: CapabilityKind::TextToVideo,
            model: "kling-v2-request".into(),
            prompt: "test".into(),
            negative_prompt: None,
            reference_image_path: None,
            reference_image_url: None,
            parameters: Default::default(),
        };
        assert_eq!(
            adapter.effective_model(&req, &credential),
            "kling-v2-request"
        );

        // request 是 "default" → 用 credential 的
        let req_default = UnifiedRequest {
            model: "default".into(),
            ..req.clone()
        };
        assert_eq!(
            adapter.effective_model(&req_default, &credential),
            "kling-v2-credential"
        );

        // credential 也空 → 用 config 默认值
        let empty_cred = CredentialContext {
            model: String::new(),
            ..credential.clone()
        };
        assert_eq!(
            adapter.effective_model(&req_default, &empty_cred),
            "kling-v1"
        );
    }
}
