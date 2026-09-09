//! 账号健康检查后台 Worker。
//!
//! 定期轮询所有活跃 resource_accounts，调用对应 Connector 的 health_check()，
//! 将结果回写 status 字段，并通过 Tauri 事件通知前端刷新。
//!
//! 事件：`account://health-updated` — payload: { accountId, oldStatus, newStatus }

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::ports::resource_connector::AccountStatus;

/// 健康检查结果事件 payload。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthUpdatedEvent {
    pub account_id: String,
    pub old_status: String,
    pub new_status: String,
}

/// 账号健康检查配置。
#[derive(Debug, Clone)]
pub struct AccountHealthConfig {
    /// 检查间隔（秒）。默认 300s = 5 分钟。
    pub interval_secs: u64,
    /// 启动后首次检查延迟（秒）。
    pub initial_delay_secs: u64,
}

impl Default for AccountHealthConfig {
    fn default() -> Self {
        Self {
            interval_secs: 300,
            initial_delay_secs: 30,
        }
    }
}

/// 后台健康检查 Worker。
pub struct AccountHealthWorker {
    database_path: PathBuf,
    app_handle: Option<AppHandle>,
    config: AccountHealthConfig,
    running: Arc<Mutex<bool>>,
}

impl AccountHealthWorker {
    pub fn new(database_path: PathBuf, config: AccountHealthConfig) -> Self {
        Self {
            database_path,
            app_handle: None,
            config,
            running: Arc::new(Mutex::new(false)),
        }
    }

    pub fn set_app_handle(&mut self, handle: AppHandle) {
        self.app_handle = Some(handle);
    }

    /// 启动后台检查线程。
    pub fn start(&self) -> Result<(), String> {
        let mut running = self.running.lock().map_err(|e| e.to_string())?;
        if *running {
            return Ok(());
        }
        *running = true;
        drop(running);

        let db_path = self.database_path.clone();
        let app = self.app_handle.clone();
        let running = Arc::clone(&self.running);
        let config = self.config.clone();

        thread::spawn(move || {
            // 首次延迟，等待应用完全启动
            thread::sleep(Duration::from_secs(config.initial_delay_secs));

            loop {
                {
                    let r = running.lock().unwrap_or_else(|e| e.into_inner());
                    if !*r {
                        break;
                    }
                }

                run_health_check(&db_path, app.as_ref());

                thread::sleep(Duration::from_secs(config.interval_secs));
            }
        });

        Ok(())
    }

    /// 停止后台线程。
    #[allow(dead_code)]
    pub fn stop(&self) {
        if let Ok(mut r) = self.running.lock() {
            *r = false;
        }
    }
}

/// 执行一轮健康检查。
fn run_health_check(database_path: &Path, app: Option<&AppHandle>) {
    let conn = match rusqlite::Connection::open(database_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[AccountHealth] Failed to open database: {e}");
            return;
        }
    };

    // 检查表是否存在
    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='resource_accounts'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .unwrap_or(false);
    if !table_exists {
        return;
    }

    // 查询所有启用且未删除的账号
    let mut stmt = match conn.prepare(
        "SELECT id, provider_id, credential_key, base_url, status FROM resource_accounts WHERE enabled = 1 AND deleted_at IS NULL",
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[AccountHealth] Failed to prepare query: {e}");
            return;
        }
    };

    let accounts: Vec<(String, String, String, String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();

    for (account_id, provider_id, credential_key, _base_url, old_status) in accounts {
        let new_status =
            check_single_account(database_path, &provider_id, &credential_key, &old_status);

        if new_status != old_status {
            // 回写状态
            let now = time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default();
            let _ = conn.execute(
                "UPDATE resource_accounts SET status = ?2, last_health_check_at = ?3, updated_at = ?3 WHERE id = ?1",
                rusqlite::params![account_id, new_status, now],
            );

            eprintln!(
                "[AccountHealth] Account {account_id} ({provider_id}): {old_status} -> {new_status}"
            );

            // 推送事件
            if let Some(handle) = app {
                let _ = handle.emit(
                    "account://health-updated",
                    HealthUpdatedEvent {
                        account_id: account_id.clone(),
                        old_status: old_status.clone(),
                        new_status: new_status.clone(),
                    },
                );
            }
        } else {
            // 状态未变，仅更新 last_health_check_at
            let now = time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default();
            let _ = conn.execute(
                "UPDATE resource_accounts SET last_health_check_at = ?2 WHERE id = ?1",
                rusqlite::params![account_id, now],
            );
        }
    }
}

/// 检查单个账号的 session 有效性。
fn check_single_account(
    database_path: &Path,
    provider_id: &str,
    credential_key: &str,
    old_status: &str,
) -> String {
    let normalized = provider_id.to_lowercase();

    // 从加密保险库读取 session
    let session = match read_keychain(database_path, credential_key) {
        Ok(s) => s,
        Err(_) => return AccountStatus::NeedLogin.as_str().to_owned(),
    };

    if session.trim().is_empty() {
        return AccountStatus::NeedLogin.as_str().to_owned();
    }

    // 根据 provider 选择检查方式
    if normalized.contains("jimeng")
        || normalized.contains("dreamina")
        || normalized.contains("即梦")
    {
        check_jimeng_session(&session, old_status)
    } else {
        // 未知 provider，保持当前状态（不轻易标记为异常）
        AccountStatus::Active.as_str().to_owned()
    }
}

/// 即梦 session 有效性检查。
///
/// 原生 /web/api/media/user/info/ 端点已失效：无签名请求即使会话有效
/// 也返回整页 HTML（SPA 文本含 "login" 字样），导致有效会话被误判为
/// 需重新登录。改走 jimeng-api 代理的 /token/points：能查到积分即会话
/// 有效；代理不可达时无法验证，保持原状态（此时生成同样不可用，
/// 但不误报登录失效）。
fn check_jimeng_session(session_id: &str, old_status: &str) -> String {
    let url = format!(
        "http://127.0.0.1:{}/token/points",
        crate::connectors::resources::jimeng_connector::JIMENG_API_PROXY_PORT
    );
    let response = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .build()
        .post(&url)
        .set("Authorization", &format!("Bearer {session_id}"))
        .set("Content-Type", "application/json")
        .call();
    match response {
        Ok(resp) => {
            let body = resp.into_string().unwrap_or_default();
            // 成功：[{ token, points: { totalCredit, ... } }]
            if body.trim_start().starts_with('[') && body.contains("totalCredit") {
                AccountStatus::Active.as_str().to_owned()
            } else {
                // 代理返回错误对象：登录失效、会话无效等
                AccountStatus::NeedLogin.as_str().to_owned()
            }
        }
        Err(_) => old_status.to_owned(),
    }
}

/// 从本地加密保险库读取密钥。
fn read_keychain(database_path: &Path, key: &str) -> Result<String, String> {
    let vault = crate::adapters::file_vault::FileVault::from_database_path(database_path)
        .map_err(|e| e.to_string())?;
    vault.get_secret(key).map_err(|e| e.to_string())
}
