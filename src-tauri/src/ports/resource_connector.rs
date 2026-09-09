//! 资源连接器端口。
//!
//! ResourceConnector 是面向「用户账号」的连接器抽象，与 UnifiedProviderAdapter（面向 API Key）互补。
//! 账号类资源（即梦、可灵、Midjourney）通过 session/cookie 认证，需要额外的登录、刷新、健康检测能力。
//!
//! 设计原则：
//! - Connector 无状态，不持有密钥（与 UnifiedProviderAdapter 一致）
//! - 登录流程由前端 WebView 完成，Connector 只负责「接收 session → 验证 → 存储 → 使用」
//! - submit/poll 复用 UnifiedRequest/UnifiedSubmitResult/UnifiedPollResult DTO，
//!   使 Generation Engine 无需区分 API 调用和账号调用

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::domain::credentials::CredentialContext;
use crate::domain::providers::ProviderError;
use crate::ports::unified_provider::{
    CapabilityKind, UnifiedDownloadResult, UnifiedPollResult, UnifiedRequest, UnifiedSubmitResult,
};

// ── 账号健康状态 ──

/// 账号连接状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    /// 正常可用
    Active,
    /// 需要重新登录（session 过期）
    NeedLogin,
    /// 已过期（token 失效且无法刷新）
    Expired,
    /// 被风控/封禁
    Blocked,
    /// 未知（尚未检测）
    Unknown,
}

impl AccountStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::NeedLogin => "need_login",
            Self::Expired => "expired",
            Self::Blocked => "blocked",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "active" => Self::Active,
            "need_login" => Self::NeedLogin,
            "expired" => Self::Expired,
            "blocked" => Self::Blocked,
            _ => Self::Unknown,
        }
    }

    /// 是否可用于生成。
    pub fn is_usable(&self) -> bool {
        matches!(self, Self::Active)
    }
}

/// 账号健康检查结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountHealth {
    /// 账号状态。
    pub status: AccountStatus,
    /// 诊断信息。
    pub message: String,
    /// 剩余积分/额度（如果平台提供）。
    pub credits_remaining: Option<u64>,
    /// 会员等级/名称（如果平台提供）。
    pub membership: Option<String>,
}

// ── 登录会话 ──

/// 登录完成后返回的会话数据，由前端 WebView 登录流程产出。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginSession {
    /// 原始 Cookie 字符串（完整 header 值）。
    pub cookies: String,
    /// Access Token（如果平台使用 token 认证）。
    pub access_token: Option<String>,
    /// Refresh Token（可选）。
    pub refresh_token: Option<String>,
    /// 设备指纹/UA 信息（部分平台校验）。
    pub device_info: Option<String>,
    /// 用户显示名（登录后获取）。
    pub display_name: Option<String>,
}

impl LoginSession {
    /// 序列化为 Keychain 存储的 JSON 字符串。
    pub fn to_keychain_json(&self) -> String {
        serde_json::json!({
            "cookies": self.cookies,
            "access_token": self.access_token,
            "refresh_token": self.refresh_token,
            "device_info": self.device_info,
        })
        .to_string()
    }
}

// ── ResourceConnector Trait ──

/// 用户账号资源连接器。
///
/// 与 UnifiedProviderAdapter 的区别：
/// - 多了 login / refresh_session 生命周期管理
/// - health_check 返回 AccountHealth（含积分/会员信息）
/// - submit/poll/download 签名与 UnifiedProviderAdapter 完全一致，
///   使 Generation Engine 可以统一调度
#[allow(dead_code)]
pub trait ResourceConnector: Send + Sync {
    /// 资源提供者标识（"jimeng" / "kling_account" / "midjourney"）。
    fn provider_id(&self) -> &str;

    /// 人类可读的提供者名称。
    fn display_name(&self) -> &str;

    /// 支持的能力列表。
    fn capabilities(&self) -> Vec<CapabilityKind>;

    /// 登录页面的 URL（前端 WebView 加载此地址）。
    fn login_url(&self) -> &str;

    /// 验证登录会话是否有效。
    /// 前端完成登录后调用此方法验证 session，成功后才存入 Keychain。
    fn validate_session(&self, session: &LoginSession) -> Result<AccountHealth, ProviderError>;

    /// 检查账号健康状态（session 是否仍然有效）。
    fn health_check(&self, credential: &CredentialContext) -> Result<AccountHealth, ProviderError>;

    /// 尝试刷新 session（如果平台支持 refresh_token）。
    /// 返回 None 表示不支持刷新，需要重新登录。
    fn refresh_session(&self, credential: &CredentialContext) -> Option<LoginSession> {
        let _ = credential;
        None
    }

    /// 提交生成请求。签名与 UnifiedProviderAdapter::submit 一致。
    fn submit(
        &self,
        request: &UnifiedRequest,
        credential: &CredentialContext,
    ) -> Result<UnifiedSubmitResult, ProviderError>;

    /// 轮询任务状态。签名与 UnifiedProviderAdapter::poll 一致。
    fn poll(
        &self,
        remote_job_id: &str,
        credential: &CredentialContext,
    ) -> Result<UnifiedPollResult, ProviderError>;

    /// 下载结果。签名与 UnifiedProviderAdapter::download 一致。
    fn download(
        &self,
        result_url: &str,
        target_dir: &Path,
        credential: &CredentialContext,
    ) -> Result<UnifiedDownloadResult, ProviderError>;

    /// 是否异步。
    fn is_async(&self) -> bool {
        true
    }
}

// ── 资源账号记录 ──

/// 资源账号数据库记录（对应 resource_accounts 表）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceAccountRecord {
    pub id: String,
    pub provider_id: String,
    /// 账号类型：session_cookie / browser_session
    pub account_type: String,
    pub display_name: String,
    /// 账号状态：active / need_login / expired / blocked / unknown
    pub status: String,
    /// OS Keychain 引用键。
    pub credential_key: String,
    pub base_url: String,
    /// 额外元数据 JSON（积分、会员等级等）。
    pub extra_json: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_health_check_at: Option<String>,
}

impl ResourceAccountRecord {
    /// 账号状态枚举。
    pub fn account_status(&self) -> AccountStatus {
        AccountStatus::parse(&self.status)
    }

    /// 是否可用于生成。
    pub fn is_usable(&self) -> bool {
        self.enabled && self.account_status().is_usable()
    }
}
