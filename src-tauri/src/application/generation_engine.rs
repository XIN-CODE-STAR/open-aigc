//! Provider 注册引擎。
//!
//! 扫描已启用的凭据记录，按 provider_name 匹配注册对应的无状态适配器。
//! Phase A 重构：适配器不持有密钥，credential 记录 ID 随适配器一起注册到 Registry，
//! 调用时由 Registry 通过 CredentialManager 解析 CredentialContext。
//!
//! Phase D2+ 扩展：同时扫描 resource_accounts 表，将账号类连接器（即梦等）
//! 通过 ConnectorAdapterBridge 桥接为 UnifiedProviderAdapter 注册到同一 Registry。

use std::path::Path;
use std::sync::Arc;

use crate::{
    adapters::{
        providers::{
            grok_unified::{GrokUnifiedAdapter, GrokUnifiedConfig},
            kling_video::{KlingConfig, KlingVideoAdapter},
            seedance_video::{SeedanceConfig, SeedanceVideoAdapter},
        },
        sqlite::credential_repository::SqliteCredentialRepository,
    },
    application::{
        account_scheduler::{AccountEntry, AccountScheduler},
        credential_service::CredentialService,
        error::AppError,
        provider_registry::ProviderRegistry,
    },
    connectors::{
        bridge::ConnectorAdapterBridge,
        resources::jimeng_connector::{JimengConfig, JimengConnector},
    },
    domain::credentials::CredentialRecord,
};

/// 扫描已启用凭据，注册对应的无状态 Provider 适配器。
/// 同时扫描 resource_accounts 表，注册账号类连接器。
///
/// 返回成功注册的 Provider 数量。
pub fn register_configured_generation_providers(
    registry: &ProviderRegistry,
    database_path: &Path,
    scheduler: &AccountScheduler,
) -> Result<usize, AppError> {
    registry.clear();

    let credential_repository = SqliteCredentialRepository::open(database_path)?;
    let credential_service =
        CredentialService::new(credential_repository, database_path.to_path_buf());

    // ── Phase 1: API Key 类凭据 → Adapter ──
    for credential in credential_service
        .list()?
        .iter()
        .filter(|credential| credential.enabled)
    {
        if is_grok_credential(credential) {
            // 无状态注册：不读取 secret，只关联 credential 记录 ID
            registry.register_with_credential(
                Arc::new(GrokUnifiedAdapter::new(GrokUnifiedConfig {
                    base_url: credential.base_url.clone(),
                    model: credential.model_name.clone(),
                })),
                credential.id.clone(),
            );
            continue;
        }

        if is_kling_credential(credential) {
            registry.register_with_credential(
                Arc::new(KlingVideoAdapter::new(KlingConfig {
                    base_url: credential.base_url.clone(),
                    default_model: if credential.model_name.is_empty() {
                        "kling-v1".into()
                    } else {
                        credential.model_name.clone()
                    },
                })),
                credential.id.clone(),
            );
            continue;
        }

        if is_seedance_credential(credential) {
            registry.register_with_credential(
                Arc::new(SeedanceVideoAdapter::new(SeedanceConfig {
                    base_url: if credential.base_url.is_empty() {
                        "https://ark.cn-beijing.volces.com".into()
                    } else {
                        credential.base_url.clone()
                    },
                    default_video_model: if credential.model_name.is_empty() {
                        "seedance-2-0-250601".into()
                    } else {
                        credential.model_name.clone()
                    },
                    default_image_model: "seedream-4-0-250601".into(),
                })),
                credential.id.clone(),
            );
            continue;
        }

        // 账号类凭据（session_cookie / browser_session）→ Connector 桥接
        if is_jimeng_account_credential(credential) {
            let connector = Arc::new(JimengConnector::new(JimengConfig {
                base_url: if credential.base_url.is_empty() {
                    "https://jimeng.jianying.com".into()
                } else {
                    credential.base_url.clone()
                },
                ..JimengConfig::default()
            }));
            registry.register_with_credential(
                Arc::new(ConnectorAdapterBridge::new(connector)),
                credential.id.clone(),
            );
        }
    }

    // ── Phase 2: 账号类资源 → Connector → Bridge → Adapter ──
    register_resource_account_connectors(registry, database_path, scheduler)?;

    Ok(registry.count())
}

/// 扫描 resource_accounts 表，将已启用的账号连接器注册到 Registry 和 Scheduler。
fn register_resource_account_connectors(
    registry: &ProviderRegistry,
    database_path: &Path,
    scheduler: &AccountScheduler,
) -> Result<(), AppError> {
    let conn = rusqlite::Connection::open(database_path).map_err(|e| {
        AppError::Provider(crate::domain::providers::ProviderError::ConfigInvalid(
            format!("打开数据库失败: {e}"),
        ))
    })?;

    // 检查表是否存在（兼容尚未迁移的旧数据库）
    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='resource_accounts'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .unwrap_or(false);

    if !table_exists {
        return Ok(());
    }

    let mut stmt = conn
        .prepare(
            "SELECT id, provider_id, base_url FROM resource_accounts WHERE enabled = 1 AND deleted_at IS NULL AND status IN ('active', 'need_login')",
        )
        .map_err(|e| {
            AppError::Provider(crate::domain::providers::ProviderError::ConfigInvalid(format!(
                "查询 resource_accounts 失败: {e}"
            )))
        })?;

    let accounts = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| {
            AppError::Provider(crate::domain::providers::ProviderError::ConfigInvalid(
                format!("遍历 resource_accounts 失败: {e}"),
            ))
        })?;

    for account in accounts.flatten() {
        let (account_id, provider_id, base_url) = account;
        let normalized = provider_id.to_lowercase();
        eprintln!("[Engine] Resource account: id={account_id}, provider={provider_id}, base_url={base_url}, normalized={normalized}, contains_jimeng={}", normalized.contains("jimeng"));

        if normalized.contains("jimeng")
            || normalized.contains("\u{5373}\u{68a6}") // 即梦
            || normalized.contains("dreamina")
        {
            let connector = Arc::new(JimengConnector::new(JimengConfig {
                base_url: if base_url.is_empty() {
                    "https://jimeng.jianying.com".into()
                } else {
                    base_url
                },
                ..JimengConfig::default()
            }));
            let bridge = Arc::new(ConnectorAdapterBridge::new(connector));
            eprintln!("[Engine] Registering jimeng provider for account {account_id}");
            registry.register_with_credential(bridge, account_id.clone());
            eprintln!(
                "[Engine] Jimeng registered OK, registry count={}",
                registry.count()
            );
            // 同步注册到多账号调度器
            scheduler.add_account(
                "jimeng",
                AccountEntry {
                    account_id: account_id.clone(),
                    credential_key: format!("resource_account:{account_id}"),
                    priority: 0,
                    consecutive_failures: 0,
                    degraded: false,
                },
            );
        }
        // 后续扩展：kling_account, midjourney 等
    }

    Ok(())
}

fn is_grok_credential(credential: &CredentialRecord) -> bool {
    credential_matches_any(credential, &["grok", "xai"])
}

fn is_kling_credential(credential: &CredentialRecord) -> bool {
    credential_matches_any(
        credential,
        &[
            "kling",
            "\u{53ef}\u{7075}", // Ke Ling
            "\u{5feb}\u{5f71}", // Kuai Ying
        ],
    )
}

fn is_seedance_credential(credential: &CredentialRecord) -> bool {
    credential_matches_any(
        credential,
        &[
            "seedance",
            "seedream",
            "volcengine",
            "ark",
            "\u{706b}\u{5c71}", // Huo Shan
            "\u{8c46}\u{5305}", // Dou Bao
            "\u{5373}\u{68a6}", // Ji Meng
        ],
    )
}

/// 即梦账号类凭据：名称匹配 + credential_type 为 session 类型。
fn is_jimeng_account_credential(credential: &CredentialRecord) -> bool {
    use crate::domain::credentials::CredentialType;
    let is_session_type = matches!(
        credential.credential_type,
        CredentialType::SessionCookie | CredentialType::BrowserSession
    );
    if !is_session_type {
        return false;
    }
    credential_matches_any(
        credential,
        &[
            "jimeng",
            "dreamina",
            "\u{5373}\u{68a6}", // 即梦
        ],
    )
}

fn credential_matches_any(credential: &CredentialRecord, needles: &[&str]) -> bool {
    let haystack = format!(
        "{} {} {}",
        credential.provider_name, credential.display_name, credential.model_name
    )
    .to_lowercase();
    needles.iter().any(|needle| haystack.contains(needle))
}
