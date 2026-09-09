//! Phase A: CredentialManager — 将 CredentialRecord + keychain secret 解析为 Provider 可消费的 CredentialContext。
//!
//! 调用链：GenerationFacade / PollWorker → CredentialManager.resolve() → CredentialContext → Provider.submit()
//!
//! Provider 永远不知道存储结构（SQLite / keychain），只看到类型化的 CredentialContext。

// Phase A: 尚未接入运行时，A3-A5 步骤完成后移除。
#![allow(dead_code)]

use rusqlite::OptionalExtension;

use crate::{
    application::credential_service::CredentialService,
    application::error::AppError,
    domain::credentials::{CredentialContext, CredentialRecord, CredentialType},
};

pub struct CredentialManager {
    service: CredentialService,
}

impl CredentialManager {
    pub fn new(service: CredentialService) -> Self {
        Self { service }
    }

    /// 根据 credential_id 解析出 Provider 可用的 CredentialContext。
    /// 每次调用都从 keychain 读取密钥（安全优先，不缓存明文）。
    /// 支持 provider_credentials 和 resource_accounts 两种来源。
    pub fn resolve(&self, credential_id: &str) -> Result<CredentialContext, AppError> {
        eprintln!("[CredentialManager] resolve: credential_id={credential_id}");

        // 1. 先查 provider_credentials
        if let Ok(Some(record)) = self.service.get(credential_id) {
            eprintln!(
                "[CredentialManager] found in provider_credentials: {}",
                record.provider_name
            );
            return self.build_context(&record);
        }

        // 2. 回退：查 resource_accounts（Session 类账号，如即梦）
        eprintln!("[CredentialManager] not in provider_credentials, trying resource_accounts");
        if let Some(ctx) = self.resolve_resource_account(credential_id)? {
            eprintln!(
                "[CredentialManager] found in resource_accounts: provider={}",
                ctx.provider_id
            );
            return Ok(ctx);
        }

        eprintln!("[CredentialManager] not found anywhere");
        Err(AppError::NotFound(format!("credential {credential_id}")))
    }

    /// 从 resource_accounts 表查找账号，读取 keychain 中的 session secret。
    fn resolve_resource_account(
        &self,
        account_id: &str,
    ) -> Result<Option<CredentialContext>, AppError> {
        let db_path = self.service.database_path();
        eprintln!("[CredentialManager] resolve_resource_account: id={account_id}, db={db_path:?}");
        let conn = rusqlite::Connection::open(db_path)
            .map_err(|e| AppError::new("open db for resource_account", e))?;

        let mut stmt = conn
            .prepare(
                "SELECT provider_id, credential_key, base_url FROM resource_accounts
                 WHERE id = ?1 AND enabled = 1 AND deleted_at IS NULL",
            )
            .map_err(|e| AppError::new("prepare resource_account query", e))?;

        let result: Option<(String, String, String)> = stmt
            .query_row(rusqlite::params![account_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2).unwrap_or_default()))
            })
            .optional()
            .map_err(|e| AppError::new("query resource_account", e))?;

        let Some((provider_id, credential_key, base_url)) = result else {
            eprintln!("[CredentialManager] resource_account not found in DB");
            return Ok(None);
        };
        eprintln!("[CredentialManager] resource_account found: provider={provider_id}, key={credential_key}, base_url={base_url}");

        let secret = match self.service.get_secret(&credential_key) {
            Ok(s) => {
                eprintln!("[CredentialManager] keychain secret length: {}", s.len());
                s
            }
            Err(e) => {
                eprintln!("[CredentialManager] keychain get_secret failed: {e:?}");
                return Err(e);
            }
        };
        // 即梦 connector 读取 cookies 中的 sessionid 或 access_token，
        // payload 必须用这两个 key，否则 connector 会报"缺少 sessionid"。
        // secret 可能是纯 sessionid 值，也可能是完整 cookie 字符串。
        let payload = serde_json::from_str::<serde_json::Value>(&secret).unwrap_or_else(|_| {
            if secret.contains('=') {
                // 完整 cookie 字符串（如 "sessionid=xxx; acw_tc=yyy; ..."）
                serde_json::json!({ "cookies": secret })
            } else {
                // 纯 sessionid 值
                serde_json::json!({
                    "cookies": format!("sessionid={secret}"),
                    "access_token": secret
                })
            }
        });

        Ok(Some(CredentialContext {
            provider_id,
            credential_type: CredentialType::SessionCookie,
            payload,
            base_url,
            model: String::new(),
        }))
    }

    /// 根据 provider_name + model 查找匹配的 enabled credential 并解析。
    /// 匹配规则：provider_name 包含匹配（忽略大小写），model_name 精确匹配或为空。
    pub fn resolve_for_provider(
        &self,
        provider_name: &str,
        model: &str,
    ) -> Result<CredentialContext, AppError> {
        let credentials = self.service.list()?;
        let provider_lower = provider_name.to_lowercase();

        let matched = credentials
            .iter()
            .find(|c| {
                c.enabled
                    && c.provider_name.to_lowercase().contains(&provider_lower)
                    && (model.is_empty() || c.model_name == model || c.model_name.is_empty())
            })
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "no enabled credential for provider={provider_name}, model={model}"
                ))
            })?;

        self.build_context(matched)
    }

    /// 列出所有 enabled credentials（不含密钥）。
    pub fn list_available(&self) -> Result<Vec<CredentialRecord>, AppError> {
        let all = self.service.list()?;
        Ok(all.into_iter().filter(|c| c.enabled).collect())
    }

    /// 内部：从 CredentialRecord + keychain 构建 CredentialContext。
    fn build_context(&self, record: &CredentialRecord) -> Result<CredentialContext, AppError> {
        let payload = match record.credential_type {
            CredentialType::ApiKey => {
                let secret = self.service.get_secret(&record.credential_key)?;
                serde_json::json!({ "api_key": secret })
            }
            CredentialType::AccessSecret => {
                // AK/SK 存储格式：keychain 中存 JSON {"access_key":"...","secret_key":"..."}
                let secret = self.service.get_secret(&record.credential_key)?;
                match serde_json::from_str::<serde_json::Value>(&secret) {
                    Ok(v) if v.get("access_key").is_some() => v,
                    // 兼容：如果不是 JSON，视为单一 key（向后兼容旧数据）
                    _ => serde_json::json!({ "access_key": secret, "secret_key": "" }),
                }
            }
            CredentialType::OAuthToken => {
                let secret = self.service.get_secret(&record.credential_key)?;
                match serde_json::from_str::<serde_json::Value>(&secret) {
                    Ok(v) => v,
                    Err(_) => serde_json::json!({ "access_token": secret }),
                }
            }
            CredentialType::SessionCookie | CredentialType::BrowserSession => {
                // 账号登录态：keychain 中存 JSON {"cookies":"...","access_token":"...","refresh_token":"...","device_info":"..."}
                let secret = self.service.get_secret(&record.credential_key)?;
                match serde_json::from_str::<serde_json::Value>(&secret) {
                    Ok(v) => v,
                    Err(_) => serde_json::json!({ "cookies": secret }),
                }
            }
            CredentialType::LocalEndpoint => {
                // 本地端点无密钥
                serde_json::json!({})
            }
        };

        Ok(CredentialContext {
            provider_id: record.provider_name.clone(),
            credential_type: record.credential_type,
            payload,
            base_url: record.base_url.clone(),
            model: record.model_name.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::credentials::{CredentialScope, CredentialType};

    fn make_record(provider: &str, cred_type: CredentialType) -> CredentialRecord {
        CredentialRecord {
            id: "test-id".to_owned(),
            provider_name: provider.to_owned(),
            display_name: "Test".to_owned(),
            base_url: "https://api.test.com".to_owned(),
            model_name: "test-model".to_owned(),
            credential_key: "credential:test-id".to_owned(),
            enabled: true,
            credential_type: cred_type,
            scope: CredentialScope::User,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn credential_context_api_key_extraction() {
        let ctx = CredentialContext {
            provider_id: "openai".to_owned(),
            credential_type: CredentialType::ApiKey,
            payload: serde_json::json!({ "api_key": "sk-test123" }),
            base_url: "https://api.openai.com".to_owned(),
            model: "gpt-image-1".to_owned(),
        };
        assert_eq!(ctx.api_key(), Some("sk-test123"));
        assert_eq!(ctx.access_key(), None);
    }

    #[test]
    fn credential_context_access_secret_extraction() {
        let ctx = CredentialContext {
            provider_id: "seedance".to_owned(),
            credential_type: CredentialType::AccessSecret,
            payload: serde_json::json!({ "access_key": "AK123", "secret_key": "SK456" }),
            base_url: "https://api.seedance.com".to_owned(),
            model: "seedance-v2".to_owned(),
        };
        assert_eq!(ctx.access_key(), Some("AK123"));
        assert_eq!(ctx.secret_key(), Some("SK456"));
        assert_eq!(ctx.api_key(), None);
    }

    #[test]
    fn credential_type_parse_roundtrip() {
        assert_eq!(CredentialType::parse("api_key"), CredentialType::ApiKey);
        assert_eq!(
            CredentialType::parse("access_secret"),
            CredentialType::AccessSecret
        );
        assert_eq!(
            CredentialType::parse("oauth_token"),
            CredentialType::OAuthToken
        );
        assert_eq!(
            CredentialType::parse("local_endpoint"),
            CredentialType::LocalEndpoint
        );
        assert_eq!(CredentialType::parse("unknown"), CredentialType::ApiKey);

        assert_eq!(CredentialType::AccessSecret.as_str(), "access_secret");
    }

    #[test]
    fn credential_record_has_default_type() {
        let record = make_record("grok", CredentialType::default());
        assert_eq!(record.credential_type, CredentialType::ApiKey);
        assert_eq!(record.scope, CredentialScope::User);
    }
}
