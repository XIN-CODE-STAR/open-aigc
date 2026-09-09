use serde::{Deserialize, Serialize};
use thiserror::Error;

const PROVIDER_NAME_MAX_LENGTH: usize = 80;
const DISPLAY_NAME_MAX_LENGTH: usize = 120;
const URL_MAX_LENGTH: usize = 500;
const MODEL_NAME_MAX_LENGTH: usize = 120;

// ── Phase A: Credential Type System ──

/// 认证类型。按认证机制分类，不按 Provider 分类——避免枚举膨胀。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CredentialType {
    /// 单一 API Key（OpenAI, Runway, Luma, Hailuo, Vidu, Pika, DashScope）
    #[default]
    ApiKey,
    /// Access Key + Secret Key 签名认证（Seedance/火山引擎, 腾讯混元, 阿里云）
    AccessSecret,
    /// OAuth 2.0 token（Adobe Firefly, Google Vertex）
    OAuthToken,
    /// Session Cookie 认证（即梦、可灵等用户账号登录态）
    SessionCookie,
    /// 浏览器授权会话（通过内嵌 WebView 完成的授权流程）
    BrowserSession,
    /// 本地推理端点（ComfyUI, Ollama）— 无密钥，仅 URL
    LocalEndpoint,
}

#[allow(dead_code)] // Phase A: as_str() 将在 A3-A5 接入运行时后使用
impl CredentialType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::AccessSecret => "access_secret",
            Self::OAuthToken => "oauth_token",
            Self::SessionCookie => "session_cookie",
            Self::BrowserSession => "browser_session",
            Self::LocalEndpoint => "local_endpoint",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "access_secret" => Self::AccessSecret,
            "oauth_token" => Self::OAuthToken,
            "session_cookie" => Self::SessionCookie,
            "browser_session" => Self::BrowserSession,
            "local_endpoint" => Self::LocalEndpoint,
            _ => Self::ApiKey,
        }
    }
}

/// 凭据作用域。第一版仅预留字段，不实现权限隔离。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CredentialScope {
    /// 个人账号
    #[default]
    User,
    /// 项目/工作区专用账号
    Workspace,
    /// 系统级统一账号
    System,
}

#[allow(dead_code)] // Phase A: as_str() 将在 A3-A5 接入运行时后使用
impl CredentialScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Workspace => "workspace",
            Self::System => "system",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "workspace" => Self::Workspace,
            "system" => Self::System,
            _ => Self::User,
        }
    }
}

/// Provider 运行时消费的凭据视图。Provider 永远不知道存储结构。
///
/// 调用链：CredentialManager.resolve() → CredentialContext → Provider.submit()
#[allow(dead_code)] // Phase A: 将在 A3 Provider trait 签名变更后使用
#[derive(Debug, Clone)]
pub struct CredentialContext {
    pub provider_id: String,
    pub credential_type: CredentialType,
    /// 类型化的密钥载荷。Provider 按自身需要解释。
    /// - ApiKey: `{ "api_key": "sk-xxx" }`
    /// - AccessSecret: `{ "access_key": "xxx", "secret_key": "xxx" }`
    /// - OAuthToken: `{ "access_token": "xxx", "refresh_token": "..." }`
    /// - SessionCookie: `{ "cookies": "sessionid=xxx; ...", "access_token": "...", "device_info": "..." }`
    /// - BrowserSession: `{ "cookies": "...", "access_token": "...", "refresh_token": "..." }`
    /// - LocalEndpoint: `{}` (无密钥)
    pub payload: serde_json::Value,
    pub base_url: String,
    pub model: String,
}

#[allow(dead_code)] // Phase A: 便捷方法将在 Provider adapter 中使用
impl CredentialContext {
    /// 便捷方法：提取单一 API Key。
    pub fn api_key(&self) -> Option<&str> {
        self.payload.get("api_key").and_then(|v| v.as_str())
    }

    /// 便捷方法：提取 Access Key。
    pub fn access_key(&self) -> Option<&str> {
        self.payload.get("access_key").and_then(|v| v.as_str())
    }

    /// 便捷方法：提取 Secret Key。
    pub fn secret_key(&self) -> Option<&str> {
        self.payload.get("secret_key").and_then(|v| v.as_str())
    }

    /// 便捷方法：提取 Session Cookies（账号登录态）。
    pub fn cookies(&self) -> Option<&str> {
        self.payload.get("cookies").and_then(|v| v.as_str())
    }

    /// 便捷方法：提取 Access Token（OAuth / 账号会话）。
    pub fn access_token(&self) -> Option<&str> {
        self.payload.get("access_token").and_then(|v| v.as_str())
    }

    /// 便捷方法：提取 Refresh Token。
    pub fn refresh_token(&self) -> Option<&str> {
        self.payload.get("refresh_token").and_then(|v| v.as_str())
    }
}

// ── Existing Credential Record ──

/// Provider 凭据记录。密钥本身不在此结构中——通过 credential_key 引用 OS keychain。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialRecord {
    pub id: String,
    pub provider_name: String,
    pub display_name: String,
    pub base_url: String,
    pub model_name: String,
    /// OS keychain 中存储密钥的引用键。
    #[serde(rename = "credentialKey")]
    pub credential_key: String,
    pub enabled: bool,
    /// 认证类型（Phase A 新增，默认 api_key 兼容现有数据）
    #[serde(default)]
    pub credential_type: CredentialType,
    /// 作用域（Phase A 新增，默认 user）
    #[serde(default)]
    pub scope: CredentialScope,
    pub created_at: String,
    pub updated_at: String,
}

/// 创建 / 更新凭据时的草稿。secret（API key）仅在创建/更新时传入，不持久化在 CredentialRecord 中。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialDraft {
    pub provider_name: String,
    pub display_name: String,
    pub base_url: String,
    pub model_name: String,
    /// 认证类型（默认 ApiKey，账号类传 SessionCookie / BrowserSession）。
    pub credential_type: CredentialType,
}

impl CredentialDraft {
    pub fn try_new(
        provider_name: String,
        display_name: String,
        base_url: String,
        model_name: String,
    ) -> Result<Self, CredentialValidationError> {
        Self::try_new_with_type(
            provider_name,
            display_name,
            base_url,
            model_name,
            CredentialType::ApiKey,
        )
    }

    pub fn try_new_with_type(
        provider_name: String,
        display_name: String,
        base_url: String,
        model_name: String,
        credential_type: CredentialType,
    ) -> Result<Self, CredentialValidationError> {
        let provider_name =
            normalize_required(provider_name, "providerName", PROVIDER_NAME_MAX_LENGTH)?;
        let display_name =
            normalize_required(display_name, "displayName", DISPLAY_NAME_MAX_LENGTH)?;
        let base_url = normalize_required(base_url, "baseUrl", URL_MAX_LENGTH)?;
        let model_name = normalize_required(model_name, "modelName", MODEL_NAME_MAX_LENGTH)?;
        Ok(Self {
            provider_name,
            display_name,
            base_url,
            model_name,
            credential_type,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CredentialValidationError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} must be at most {max_length} characters")]
    TooLong {
        field: &'static str,
        max_length: usize,
    },
    #[error("{field} contains unsupported control characters")]
    ControlCharacters { field: &'static str },
}

impl CredentialValidationError {
    pub fn field(&self) -> Option<&'static str> {
        match self {
            Self::Required { field }
            | Self::TooLong { field, .. }
            | Self::ControlCharacters { field } => Some(field),
        }
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::Required { field } => format!("{}不能为空。", field_label(field)),
            Self::TooLong { field, max_length } => {
                format!("{}不能超过 {max_length} 个字符。", field_label(field))
            }
            Self::ControlCharacters { field } => {
                format!("{}包含不支持的控制字符。", field_label(field))
            }
        }
    }
}

fn field_label(field: &str) -> &str {
    match field {
        "providerName" => "供应商名称",
        "displayName" => "显示名称",
        "baseUrl" => "服务地址",
        "modelName" => "模型名称",
        _ => "字段",
    }
}

fn normalize_required(
    value: String,
    field: &'static str,
    max_length: usize,
) -> Result<String, CredentialValidationError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(CredentialValidationError::Required { field });
    }
    if value.chars().count() > max_length {
        return Err(CredentialValidationError::TooLong { field, max_length });
    }
    if value.chars().any(char::is_control) {
        return Err(CredentialValidationError::ControlCharacters { field });
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_draft() {
        let draft = CredentialDraft::try_new(
            "Seedance".to_owned(),
            "种子舞蹈".to_owned(),
            "https://api.seedance.com".to_owned(),
            "seedance-v2".to_owned(),
        )
        .unwrap();
        assert_eq!(draft.provider_name, "Seedance");
        assert_eq!(draft.display_name, "种子舞蹈");
    }

    #[test]
    fn rejects_empty_provider_name() {
        let error = CredentialDraft::try_new(
            "  ".to_owned(),
            "测试".to_owned(),
            "https://api.test.com".to_owned(),
            "model".to_owned(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            CredentialValidationError::Required {
                field: "providerName"
            }
        ));
    }

    #[test]
    fn rejects_too_long_base_url() {
        let long_url = "https://api.test.com/".to_owned() + &"x".repeat(500);
        let error = CredentialDraft::try_new(
            "Seedance".to_owned(),
            "测试".to_owned(),
            long_url,
            "model".to_owned(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            CredentialValidationError::TooLong {
                field: "baseUrl",
                max_length: 500
            }
        ));
    }
}
