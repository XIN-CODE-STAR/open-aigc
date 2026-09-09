//! Seedance 视频 Provider 适配器。
//!
//! 接入火山引擎方舟平台 Seedance 视频/图片生成 API，支持：
//! - Text-to-Video（文生视频）
//! - Image-to-Video（图生视频）
//! - Text-to-Image（文生图）
//! - Image-to-Image（图生图）
//!
//! 认证方式：Bearer API Key（ARK_API_KEY），无需 AK/SK 签名。
//! API 基础地址：https://ark.cn-beijing.volces.com
//!
//! Phase D2 落地：Adapter 无状态，不持有密钥。
//! 每次调用通过 &CredentialContext 获取 api_key。

use std::path::Path;
use std::time::Duration;

use crate::domain::credentials::CredentialContext;
use crate::domain::providers::ProviderError;
use crate::ports::unified_provider::{
    CapabilityKind, UnifiedDownloadResult, UnifiedHealthStatus, UnifiedPollResult,
    UnifiedProviderAdapter, UnifiedRequest, UnifiedSubmitResult,
};

/// Seedance 适配器配置（仅含非敏感默认值）。
#[derive(Debug, Clone)]
pub struct SeedanceConfig {
    pub base_url: String,
    /// 默认视频模型。
    pub default_video_model: String,
    /// 默认图片模型。
    pub default_image_model: String,
}

impl Default for SeedanceConfig {
    fn default() -> Self {
        Self {
            base_url: "https://ark.cn-beijing.volces.com".into(),
            default_video_model: "seedance-2-0-250601".into(),
            default_image_model: "seedream-4-0-250601".into(),
        }
    }
}

/// Seedance Provider 适配器（无状态，不持有密钥）。
pub struct SeedanceVideoAdapter {
    config: SeedanceConfig,
    agent: ureq::Agent,
}

impl SeedanceVideoAdapter {
    pub fn new(config: SeedanceConfig) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(300))
            .build();
        Self { config, agent }
    }

    /// 从 CredentialContext 提取 API Key。
    fn api_key(credential: &CredentialContext) -> Result<String, ProviderError> {
        credential
            .api_key()
            .map(|s| s.to_owned())
            .ok_or_else(|| ProviderError::ConfigInvalid("Seedance credential 缺少 api_key".into()))
    }

    /// 有效 base_url：credential 优先，回退到 config 默认值。
    fn effective_base_url(&self, credential: &CredentialContext) -> String {
        if credential.base_url.is_empty() {
            self.config.base_url.clone()
        } else {
            credential.base_url.trim_end_matches('/').to_owned()
        }
    }

    /// 有效模型名：request > credential > config 默认值（按能力区分视频/图片）。
    fn effective_model(&self, request: &UnifiedRequest, credential: &CredentialContext) -> String {
        if !request.model.is_empty() && request.model != "default" {
            return request.model.clone();
        }
        if !credential.model.is_empty() {
            return credential.model.clone();
        }
        match request.capability {
            CapabilityKind::TextToVideo | CapabilityKind::ImageToVideo => {
                self.config.default_video_model.clone()
            }
            _ => self.config.default_image_model.clone(),
        }
    }

    /// 构建 content 数组。
    fn build_content(&self, request: &UnifiedRequest) -> Vec<serde_json::Value> {
        let mut content = vec![serde_json::json!({
            "type": "text",
            "text": request.prompt,
        })];

        // 图生视频 / 图生图：附加参考图片
        if matches!(
            request.capability,
            CapabilityKind::ImageToVideo | CapabilityKind::ImageToImage
        ) {
            if let Some(url) = &request.reference_image_url {
                content.push(serde_json::json!({
                    "type": "image_url",
                    "image_url": { "url": url },
                    "role": "first_frame",
                }));
            }
        }

        content
    }

    /// 构建提交请求体。
    fn build_submit_body(
        &self,
        request: &UnifiedRequest,
        credential: &CredentialContext,
    ) -> serde_json::Value {
        let model = self.effective_model(request, credential);
        let content = self.build_content(request);

        let mut body = serde_json::json!({
            "model": model,
            "content": content,
        });

        // 视频参数
        if matches!(
            request.capability,
            CapabilityKind::TextToVideo | CapabilityKind::ImageToVideo
        ) {
            if let Some(duration) = request.parameters.get("duration") {
                body["duration"] = duration.clone();
            }
            if let Some(ratio) = request.parameters.get("ratio") {
                body["ratio"] = ratio.clone();
            } else if let Some(aspect_ratio) = request.parameters.get("aspect_ratio") {
                body["ratio"] = aspect_ratio.clone();
            }
            if let Some(resolution) = request.parameters.get("resolution") {
                body["resolution"] = resolution.clone();
            }
            if let Some(seed) = request.parameters.get("seed") {
                body["seed"] = seed.clone();
            }
            if let Some(generate_audio) = request.parameters.get("generate_audio") {
                body["generate_audio"] = generate_audio.clone();
            }
        }

        // 图片参数
        if matches!(
            request.capability,
            CapabilityKind::TextToImage | CapabilityKind::ImageToImage
        ) {
            if let Some(ratio) = request.parameters.get("ratio") {
                body["ratio"] = ratio.clone();
            } else if let Some(aspect_ratio) = request.parameters.get("aspect_ratio") {
                body["ratio"] = aspect_ratio.clone();
            }
        }

        body
    }
}

impl UnifiedProviderAdapter for SeedanceVideoAdapter {
    fn provider_id(&self) -> &str {
        "seedance"
    }

    fn capabilities(&self) -> Vec<CapabilityKind> {
        vec![
            CapabilityKind::TextToVideo,
            CapabilityKind::ImageToVideo,
            CapabilityKind::TextToImage,
            CapabilityKind::ImageToImage,
        ]
    }

    fn submit(
        &self,
        request: &UnifiedRequest,
        credential: &CredentialContext,
    ) -> Result<UnifiedSubmitResult, ProviderError> {
        let api_key = Self::api_key(credential)?;
        let base_url = self.effective_base_url(credential);
        let url = format!("{base_url}/api/v3/contents/generations/tasks");
        let body = self.build_submit_body(request, credential);

        let resp: serde_json::Value = self
            .agent
            .post(&url)
            .set("Authorization", &format!("Bearer {api_key}"))
            .set("Content-Type", "application/json")
            .send_json(&body)
            .map_err(classify_seedance_error)?
            .into_json()
            .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

        // 方舟平台错误响应：{ "error": { "code": "...", "message": "..." } }
        if let Some(error) = resp.get("error") {
            return Err(ProviderError::Remote {
                status: 400,
                body: error["message"]
                    .as_str()
                    .unwrap_or("unknown error")
                    .to_owned(),
            });
        }

        let task_id = resp["id"]
            .as_str()
            .ok_or_else(|| ProviderError::MalformedResponse("响应缺少任务 id 字段".into()))?;

        Ok(UnifiedSubmitResult {
            remote_job_id: task_id.to_owned(),
            initial_status: "queued".to_owned(),
            immediate_result_url: None,
            estimated_duration_secs: Some(180),
        })
    }

    fn poll(
        &self,
        remote_job_id: &str,
        credential: &CredentialContext,
    ) -> Result<UnifiedPollResult, ProviderError> {
        let api_key = Self::api_key(credential)?;
        let base_url = self.effective_base_url(credential);
        let url = format!("{base_url}/api/v3/contents/generations/tasks/{remote_job_id}");

        let resp: serde_json::Value = self
            .agent
            .get(&url)
            .set("Authorization", &format!("Bearer {api_key}"))
            .call()
            .map_err(classify_seedance_error)?
            .into_json()
            .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

        if let Some(error) = resp.get("error") {
            return Err(ProviderError::Remote {
                status: 400,
                body: error["message"]
                    .as_str()
                    .unwrap_or("unknown error")
                    .to_owned(),
            });
        }

        let status = resp["status"].as_str().unwrap_or("running");
        match status {
            "succeeded" => {
                // 视频结果在 content 数组中
                let result_url = resp["content"]
                    .as_array()
                    .and_then(|arr| {
                        arr.iter().find_map(|item| {
                            item["video_url"]["url"]
                                .as_str()
                                .or_else(|| item["image_url"]["url"].as_str())
                                .or_else(|| item["url"].as_str())
                        })
                    })
                    .or_else(|| resp["video_url"].as_str())
                    .map(|s| s.to_owned());

                Ok(UnifiedPollResult {
                    status: "succeeded".to_owned(),
                    progress: 100,
                    result_url,
                    error_message: None,
                    retryable: false,
                })
            }
            "failed" | "expired" => {
                let message = resp["error"]["message"]
                    .as_str()
                    .or_else(|| resp["message"].as_str())
                    .unwrap_or("任务失败")
                    .to_owned();
                Ok(UnifiedPollResult {
                    status: "failed".to_owned(),
                    progress: 0,
                    result_url: None,
                    error_message: Some(message),
                    retryable: status == "failed",
                })
            }
            // queued / running
            _ => {
                let progress = if status == "queued" { 10 } else { 50 };
                Ok(UnifiedPollResult {
                    status: "processing".to_owned(),
                    progress,
                    result_url: None,
                    error_message: None,
                    retryable: false,
                })
            }
        }
    }

    fn download(
        &self,
        result_url: &str,
        target_dir: &Path,
        _credential: &CredentialContext,
    ) -> Result<UnifiedDownloadResult, ProviderError> {
        // 下载结果文件不需要认证（URL 自带签名/时效）
        let resp = self
            .agent
            .get(result_url)
            .call()
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let content_type = resp
            .header("Content-Type")
            .unwrap_or("video/mp4")
            .to_owned();
        let extension = if content_type.contains("image") {
            "png"
        } else {
            "mp4"
        };
        let file_name = format!("seedance-{}.{}", uuid::Uuid::new_v4(), extension);
        let file_path = target_dir.join(&file_name);

        let mut body = resp.into_reader();
        let mut file =
            std::fs::File::create(&file_path).map_err(|e| ProviderError::Network(e.to_string()))?;
        std::io::copy(&mut body, &mut file).map_err(|e| ProviderError::Network(e.to_string()))?;

        let file_size = std::fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0);
        let mime_type = if extension == "png" {
            "image/png".to_owned()
        } else {
            "video/mp4".to_owned()
        };

        Ok(UnifiedDownloadResult {
            file_path: file_path.to_string_lossy().to_string(),
            mime_type,
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
        // 用 models 列表接口做轻量健康检查
        match self
            .agent
            .get(&format!("{base_url}/api/v3/models"))
            .set("Authorization", &format!("Bearer {api_key}"))
            .call()
        {
            Ok(_) => Ok(UnifiedHealthStatus {
                available: true,
                message: "Seedance API 可用".to_owned(),
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
                let duration = request
                    .parameters
                    .get("duration")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(5.0);
                // Seedance 2.0 约 0.5 元/5s
                Some(0.5 * (duration / 5.0))
            }
            CapabilityKind::TextToImage | CapabilityKind::ImageToImage => Some(0.04),
            _ => None,
        }
    }

    fn is_async(&self) -> bool {
        true
    }
}

// ─────────────────────────────────────────────────────
// 工具函数
// ─────────────────────────────────────────────────────

fn classify_seedance_error(error: ureq::Error) -> ProviderError {
    let message = error.to_string();
    match error {
        ureq::Error::Status(status, response) => {
            let body = response.into_string().unwrap_or_default();
            ProviderError::Remote {
                status: status as u16,
                body,
            }
        }
        _ => ProviderError::Network(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::credentials::CredentialType;

    fn test_credential() -> CredentialContext {
        CredentialContext {
            provider_id: "seedance".into(),
            credential_type: CredentialType::ApiKey,
            payload: serde_json::json!({
                "api_key": "test-ark-api-key"
            }),
            base_url: String::new(),
            model: String::new(),
        }
    }

    #[test]
    fn seedance_adapter_capabilities() {
        let adapter = SeedanceVideoAdapter::new(SeedanceConfig::default());
        assert_eq!(adapter.provider_id(), "seedance");
        let caps = adapter.capabilities();
        assert!(caps.contains(&CapabilityKind::TextToVideo));
        assert!(caps.contains(&CapabilityKind::ImageToVideo));
        assert!(caps.contains(&CapabilityKind::TextToImage));
        assert!(caps.contains(&CapabilityKind::ImageToImage));
        assert!(!caps.contains(&CapabilityKind::TextToSpeech));
    }

    #[test]
    fn seedance_is_async() {
        let adapter = SeedanceVideoAdapter::new(SeedanceConfig::default());
        assert!(adapter.is_async());
    }

    #[test]
    fn seedance_api_key_extraction() {
        let credential = test_credential();
        let key = SeedanceVideoAdapter::api_key(&credential).unwrap();
        assert_eq!(key, "test-ark-api-key");
    }

    #[test]
    fn seedance_api_key_missing() {
        let empty = CredentialContext {
            provider_id: "seedance".into(),
            credential_type: CredentialType::ApiKey,
            payload: serde_json::json!({}),
            base_url: String::new(),
            model: String::new(),
        };
        assert!(SeedanceVideoAdapter::api_key(&empty).is_err());
    }

    #[test]
    fn seedance_effective_model_fallback() {
        let adapter = SeedanceVideoAdapter::new(SeedanceConfig::default());
        let credential = test_credential();

        // request 有 model → 用 request 的
        let req = UnifiedRequest {
            capability: CapabilityKind::TextToVideo,
            model: "seedance-1-5-pro".into(),
            prompt: "test".into(),
            negative_prompt: None,
            reference_image_path: None,
            reference_image_url: None,
            parameters: Default::default(),
        };
        assert_eq!(
            adapter.effective_model(&req, &credential),
            "seedance-1-5-pro"
        );

        // request model 为 "default" → 用 config 默认视频模型
        let req_default = UnifiedRequest {
            model: "default".into(),
            ..req.clone()
        };
        assert_eq!(
            adapter.effective_model(&req_default, &credential),
            "seedance-2-0-250601"
        );

        // 图片能力 → 用 config 默认图片模型
        let req_image = UnifiedRequest {
            capability: CapabilityKind::TextToImage,
            model: "default".into(),
            ..req.clone()
        };
        assert_eq!(
            adapter.effective_model(&req_image, &credential),
            "seedream-4-0-250601"
        );
    }

    #[test]
    fn seedance_build_content_text_to_video() {
        let adapter = SeedanceVideoAdapter::new(SeedanceConfig::default());
        let req = UnifiedRequest {
            capability: CapabilityKind::TextToVideo,
            model: String::new(),
            prompt: "一只猫在跳舞".into(),
            negative_prompt: None,
            reference_image_path: None,
            reference_image_url: None,
            parameters: Default::default(),
        };
        let content = adapter.build_content(&req);
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "一只猫在跳舞");
    }

    #[test]
    fn seedance_build_content_image_to_video() {
        let adapter = SeedanceVideoAdapter::new(SeedanceConfig::default());
        let req = UnifiedRequest {
            capability: CapabilityKind::ImageToVideo,
            model: String::new(),
            prompt: "让图片动起来".into(),
            negative_prompt: None,
            reference_image_path: None,
            reference_image_url: Some("https://example.com/cat.png".into()),
            parameters: Default::default(),
        };
        let content = adapter.build_content(&req);
        assert_eq!(content.len(), 2);
        assert_eq!(content[1]["type"], "image_url");
        assert_eq!(content[1]["role"], "first_frame");
        assert_eq!(
            content[1]["image_url"]["url"],
            "https://example.com/cat.png"
        );
    }

    #[test]
    fn seedance_estimate_cost() {
        let adapter = SeedanceVideoAdapter::new(SeedanceConfig::default());
        let req_video = UnifiedRequest {
            capability: CapabilityKind::TextToVideo,
            model: String::new(),
            prompt: "test".into(),
            negative_prompt: None,
            reference_image_path: None,
            reference_image_url: None,
            parameters: [("duration".into(), serde_json::json!(10))]
                .into_iter()
                .collect(),
        };
        let cost = adapter.estimate_cost(&req_video).unwrap();
        assert!((cost - 1.0).abs() < 0.01); // 10s → 0.5 * 2 = 1.0

        let req_image = UnifiedRequest {
            capability: CapabilityKind::TextToImage,
            ..req_video.clone()
        };
        assert_eq!(adapter.estimate_cost(&req_image), Some(0.04));
    }
}
