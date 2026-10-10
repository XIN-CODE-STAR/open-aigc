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
///
/// `reason`：`"status"` = 账号状态变化；`"credits"` = 仅积分余额刷新
/// （此时 `oldStatus == newStatus`）。UI 据此决定是否重载账号列表。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthUpdatedEvent {
    pub account_id: String,
    pub old_status: String,
    pub new_status: String,
    pub reason: &'static str,
}

/// 即梦账号积分快照（`/token/points` 返回的 `points` 字段）。
///
/// 顺带取回并落进 `resource_accounts.extra_json`，供 UI 在「生成来源」里
/// 展示账号剩余积分——此前这段数据被解析出来后直接丢掉了。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JimengCredits {
    pub gift: i64,
    pub purchase: i64,
    pub vip: i64,
    pub total: i64,
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
        let (new_status, credits) =
            check_single_account(database_path, &provider_id, &credential_key, &old_status);

        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();
        // 积分每次都写：余额变化是常态，而它不会改变账号状态。
        let credits_json = credits.map(|c| {
            serde_json::json!({
                "credits": {
                    "total": c.total,
                    "gift": c.gift,
                    "purchase": c.purchase,
                    "vip": c.vip,
                },
                "creditsCheckedAt": now,
            })
            .to_string()
        });

        if new_status != old_status {
            // 回写状态（若同时取到积分，一并写入）
            match &credits_json {
                Some(extra) => {
                    let _ = conn.execute(
                        "UPDATE resource_accounts SET status = ?2, last_health_check_at = ?3, updated_at = ?3, extra_json = ?4 WHERE id = ?1",
                        rusqlite::params![account_id, new_status, now, extra],
                    );
                }
                None => {
                    let _ = conn.execute(
                        "UPDATE resource_accounts SET status = ?2, last_health_check_at = ?3, updated_at = ?3 WHERE id = ?1",
                        rusqlite::params![account_id, new_status, now],
                    );
                }
            }

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
                        reason: "status",
                    },
                );
            }
        } else {
            // 状态未变：仍更新检查时间，并把刚取到的积分余额写回去
            match &credits_json {
                Some(extra) => {
                    let _ = conn.execute(
                        "UPDATE resource_accounts SET last_health_check_at = ?2, extra_json = ?3 WHERE id = ?1",
                        rusqlite::params![account_id, now, extra],
                    );
                    // 余额是 UI 要展示的数据，写回后通知前端重载账号列表；
                    // 此前这个事件只在状态变化时发，而前端也没人监听。
                    if let Some(handle) = app {
                        let _ = handle.emit(
                            "account://health-updated",
                            HealthUpdatedEvent {
                                account_id: account_id.clone(),
                                old_status: old_status.clone(),
                                new_status: new_status.clone(),
                                reason: "credits",
                            },
                        );
                    }
                }
                None => {
                    let _ = conn.execute(
                        "UPDATE resource_accounts SET last_health_check_at = ?2 WHERE id = ?1",
                        rusqlite::params![account_id, now],
                    );
                }
            }
        }
    }
}

/// 检查单个账号的 session 有效性，并尽量顺带取回积分余额。
///
/// 返回 `(status, credits)`：积分是附加信息，取不到不影响状态判定。
fn check_single_account(
    database_path: &Path,
    provider_id: &str,
    credential_key: &str,
    old_status: &str,
) -> (String, Option<JimengCredits>) {
    let normalized = provider_id.to_lowercase();

    // 从加密保险库读取 session
    let session = match read_keychain(database_path, credential_key) {
        Ok(s) => s,
        Err(_) => return (AccountStatus::NeedLogin.as_str().to_owned(), None),
    };

    if session.trim().is_empty() {
        return (AccountStatus::NeedLogin.as_str().to_owned(), None);
    }

    // 根据 provider 选择检查方式
    if normalized.contains("jimeng")
        || normalized.contains("dreamina")
        || normalized.contains("即梦")
    {
        check_jimeng_session(&session, old_status)
    } else {
        // 未知 provider，保持当前状态（不轻易标记为异常）
        (AccountStatus::Active.as_str().to_owned(), None)
    }
}

/// 即梦 session 有效性检查，并顺带取回积分余额。
///
/// 原生 /web/api/media/user/info/ 端点已失效：无签名请求即使会话有效
/// 也返回整页 HTML（SPA 文本含 "login" 字样），导致有效会话被误判为
/// 需重新登录。改走 jimeng-api 代理的 /token/points：能查到积分即会话
/// 有效；代理不可达时无法验证，保持原状态（此时生成同样不可用，
/// 但不误报登录失效）。
///
/// 返回 `(status, credits)`。积分是**附加信息**：解析失败只丢积分，不影响状态判定。
fn check_jimeng_session(session_id: &str, old_status: &str) -> (String, Option<JimengCredits>) {
    // jm_ 前缀是 jimeng-free-api-all 新代理的托管 API Key（账号池模式），
    // 不是即梦 sessionid：拿它查积分必然 1015，会把健康账号误标 need_login
    // （00:54 误报复盘）。其有效性由代理账号池维护，真实故障在提交时
    // 由工具如实上报，这里视为可用。
    if session_id.starts_with("jm_") {
        return (AccountStatus::Active.as_str().to_owned(), None);
    }
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
            // 成功：[{ token, points: { totalCredit, giftCredit, purchaseCredit, vipCredit } }]
            if body.trim_start().starts_with('[') && body.contains("totalCredit") {
                (
                    AccountStatus::Active.as_str().to_owned(),
                    parse_jimeng_credits(&body),
                )
            } else {
                // 代理返回错误对象：登录失效、会话无效等
                (AccountStatus::NeedLogin.as_str().to_owned(), None)
            }
        }
        Err(_) => (old_status.to_owned(), None),
    }
}

/// 从 `/token/points` 的响应里取第一个 token 的积分。解析失败返回 `None`。
fn parse_jimeng_credits(body: &str) -> Option<JimengCredits> {
    let parsed: serde_json::Value = serde_json::from_str(body).ok()?;
    let points = parsed.as_array()?.first()?.get("points")?;
    let total = points.get("totalCredit").and_then(serde_json::Value::as_i64)?;
    let num = |key: &str| {
        points
            .get(key)
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0)
    };
    Some(JimengCredits {
        gift: num("giftCredit"),
        purchase: num("purchaseCredit"),
        vip: num("vipCredit"),
        total,
    })
}

/// 从本地加密保险库读取密钥。
fn read_keychain(database_path: &Path, key: &str) -> Result<String, String> {
    let vault = crate::adapters::file_vault::FileVault::from_database_path(database_path)
        .map_err(|e| e.to_string())?;
    vault.get_secret(key).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 代理 `/token/points` 的成功响应形态（截自真实响应结构）。
    const OK_BODY: &str = r#"[{"token":"abc","points":{"giftCredit":66,"purchaseCredit":0,"vipCredit":0,"totalCredit":66}}]"#;

    #[test]
    fn parses_credits_from_points_response() {
        let credits = parse_jimeng_credits(OK_BODY).expect("应解析出积分");
        assert_eq!(credits.gift, 66);
        assert_eq!(credits.purchase, 0);
        assert_eq!(credits.vip, 0);
        assert_eq!(credits.total, 66);
    }

    #[test]
    fn missing_optional_breakdown_defaults_to_zero() {
        let body = r#"[{"token":"abc","points":{"totalCredit":12}}]"#;
        let credits = parse_jimeng_credits(body).expect("totalCredit 在就够");
        assert_eq!(credits.total, 12);
        assert_eq!(credits.gift, 0);
    }

    /// 积分是附加信息：这些形态都必须安静地返回 None，而不是 panic 或误报。
    #[test]
    fn returns_none_for_unparseable_shapes() {
        for body in [
            "",
            "not json",
            "{}",
            "[]",
            r#"[{"token":"abc"}]"#,
            r#"[{"token":"abc","points":{}}]"#,
            r#"{"error":"未登录"}"#,
        ] {
            assert!(
                parse_jimeng_credits(body).is_none(),
                "应返回 None：{body}"
            );
        }
    }
}
