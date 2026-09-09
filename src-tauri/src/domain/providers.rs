#![allow(dead_code)]
//!
//! Provider 领域模型：API Key 与账户登录型 Provider 共用的生成请求/响应/错误，
//! 以及 Phase 3 引入的认证模式、能力描述符、健康快照。
//!
//! 当新增 Provider 适配器落地时即可移除对应 `#[allow(dead_code)]` 注解。

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Provider 适配器的生成请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationRequest {
    /// 提示词。
    pub prompt: String,
    /// 模型名称。
    pub model: String,
    /// 额外参数（宽高、时长等），由各 provider 自行解析。
    #[serde(default)]
    pub parameters: serde_json::Value,
    /// 关联资产（参考图等），由 provider 适配器解析。
    #[serde(default)]
    pub reference_assets: Vec<String>,
}

/// Provider 适配器返回的提交结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSubmitResult {
    /// Provider 返回的远程任务 ID。
    pub remote_task_id: String,
    /// 任务状态（pending / running / succeeded / failed）。
    pub status: String,
}

/// Provider 适配器返回的轮询结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPollResult {
    /// 任务状态。
    pub status: String,
    /// 进度百分比 0-100。
    pub progress: u8,
    /// 完成时返回的结果文件 URL（下载链接）。
    pub result_url: Option<String>,
    /// 失败时的错误信息。
    pub error_message: Option<String>,
}

/// Provider 适配器错误。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProviderError {
    #[error("provider configuration is missing or invalid: {0}")]
    ConfigInvalid(String),
    #[error("provider network request failed: {0}")]
    Network(String),
    #[error("provider returned an error response: {status} {body}")]
    Remote { status: u16, body: String },
    #[error("provider response was malformed: {0}")]
    MalformedResponse(String),
    #[error("provider authentication failed: {0}")]
    AuthFailed(String),
    #[error("provider account is rate-limited; retry after {retry_after_seconds}s")]
    RateLimited { retry_after_seconds: u64 },
}

impl ProviderError {
    pub fn user_message(&self) -> String {
        match self {
            Self::ConfigInvalid(msg) => format!("Provider 配置无效：{}", msg),
            Self::Network(msg) => format!("网络请求失败：{}", msg),
            Self::Remote { status, body } => format!("Provider 返回错误 {}：{}", status, body),
            Self::MalformedResponse(msg) => format!("Provider 响应格式错误：{}", msg),
            Self::AuthFailed(msg) => format!("Provider 鉴权失败：{}", msg),
            Self::RateLimited {
                retry_after_seconds,
            } => {
                format!("Provider 限流，请等待 {} 秒后重试", retry_after_seconds)
            }
        }
    }
}

// ────────────────────────────────────────────────────────────────────
// Phase 3 扩展：账户登录型 Provider 架构
// ────────────────────────────────────────────────────────────────────

/// Provider 认证模式。
///
/// - `api_key`：API Key 直连（OpenAI、Seedance 等）
/// - `oauth`：OAuth 2.0 授权（如可能接入的官方账号）
/// - `account_login`：第三方账户登录（用户名/密码 + 验证码）— Phase 3 预留
/// - `session_cookie`：浏览器 session cookie — Phase 3 预留
/// - `manual`：用户手动粘贴 token/cookie — Phase 3 预留
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAuthMode {
    ApiKey,
    Oauth,
    AccountLogin,
    SessionCookie,
    Manual,
}

impl ProviderAuthMode {
    /// 是否为 API Key 模式（旧行为）。
    pub fn is_api_key(&self) -> bool {
        matches!(self, Self::ApiKey)
    }

    /// 是否需要后台维护登录态（oauth / account_login / session_cookie / manual）。
    pub fn requires_account(&self) -> bool {
        !self.is_api_key()
    }

    /// 中文描述（用于 UI 展示）。
    pub fn display_label(&self) -> &'static str {
        match self {
            Self::ApiKey => "API Key",
            Self::Oauth => "OAuth",
            Self::AccountLogin => "账户登录",
            Self::SessionCookie => "Session Cookie",
            Self::Manual => "手动 Token",
        }
    }
}

/// Provider 能力种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCapabilityKind {
    TextToImage,
    ImageToImage,
    TextToVideo,
    ImageToVideo,
    TextToSpeech,
    VoiceClone,
    AgentLoop,
    DigitalHuman,
}

/// Provider 能力定义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapability {
    pub kind: ProviderCapabilityKind,
    /// 支持的模型列表（空表示任意）。
    #[serde(default)]
    pub models: Vec<String>,
    /// 是否支持参考图。
    #[serde(default)]
    pub supports_reference: bool,
}

/// Provider 健康状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHealth {
    /// 状态：ok / degraded / expired / failed / unknown。
    pub status: ProviderHealthStatus,
    /// 用户可读诊断信息（可选）。
    #[serde(default)]
    pub message: Option<String>,
    /// 最近一次健康检查时间（RFC 3339 字符串）。
    #[serde(default)]
    pub last_checked_at: Option<String>,
    /// 距离下次需要重新登录/刷新的秒数。
    #[serde(default)]
    pub refresh_in_seconds: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderHealthStatus {
    Ok,
    Degraded,
    Expired,
    Failed,
    Unknown,
}

impl ProviderHealthStatus {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Ok | Self::Degraded)
    }
}

/// Provider 描述符（来自后端 metadata，前端 ProviderModelSelector 渲染用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDescriptor {
    /// Provider 唯一标识（与 adapter provider_id 一致）。
    pub provider_id: String,
    /// 展示名称。
    pub display_name: String,
    /// 支持的认证模式列表。
    pub auth_modes: Vec<ProviderAuthMode>,
    /// Provider 能力列表。
    pub capabilities: Vec<ProviderCapability>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_mode_classification() {
        assert!(ProviderAuthMode::ApiKey.is_api_key());
        assert!(!ProviderAuthMode::Oauth.is_api_key());
        assert!(ProviderAuthMode::AccountLogin.requires_account());
        assert!(ProviderAuthMode::Manual.requires_account());
    }

    #[test]
    fn health_status_availability() {
        assert!(ProviderHealthStatus::Ok.is_available());
        assert!(ProviderHealthStatus::Degraded.is_available());
        assert!(!ProviderHealthStatus::Expired.is_available());
        assert!(!ProviderHealthStatus::Failed.is_available());
    }

    #[test]
    fn provider_descriptor_roundtrip() {
        let desc = ProviderDescriptor {
            provider_id: "seedance".into(),
            display_name: "Seedance".into(),
            auth_modes: vec![ProviderAuthMode::ApiKey, ProviderAuthMode::AccountLogin],
            capabilities: vec![ProviderCapability {
                kind: ProviderCapabilityKind::TextToVideo,
                models: vec!["seedance-1.0".into()],
                supports_reference: true,
            }],
        };
        let json = serde_json::to_string(&desc).unwrap();
        let back: ProviderDescriptor = serde_json::from_str(&json).unwrap();
        assert_eq!(back.provider_id, "seedance");
        assert_eq!(back.auth_modes.len(), 2);
    }
}
