//! 应用级设置（`app_settings` 键值表）。
//!
//! 表本来就在（`admin_username` / `admin_password_hash` 是 2026-07-19 留下的），
//! 但 **Rust 侧此前没有任何代码读写它** —— 于是「应用级偏好」无处可存。
//! 这里补上最小可用的读写，第一个使用者是「大语言模型」（作用于当前应用的所有对话）。
//!
//! 读写都是尽力而为的轻量操作：失败时返回 `None` / 错误由调用方决定是否致命。
//! 设置不参与事务，也不缓存——每次现读，避免与外部修改（如备份恢复）不一致。

use std::path::Path;

use rusqlite::{Connection, OptionalExtension};

/// 「大语言模型」所用的凭据 id（`provider_credentials.id`）。
///
/// 设了它，**当前应用的所有对话都用这个模型**；没设则回退到各对话自带的
/// `conversation.credential_id`（旧行为）。
pub const KEY_DEFAULT_LLM_CREDENTIAL: &str = "default_llm_credential_id";

/// 读一个设置项。键不存在或数据库不可用时返回 `None`。
pub fn get_setting(database_path: &Path, key: &str) -> Option<String> {
    let conn = Connection::open(database_path).ok()?;
    conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        [key],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .filter(|value| !value.trim().is_empty())
}

/// 写一个设置项（存在则覆盖）。传空串等同于清除该项。
pub fn set_setting(database_path: &Path, key: &str, value: &str) -> Result<(), String> {
    let conn = Connection::open(database_path).map_err(|e| e.to_string())?;
    if value.trim().is_empty() {
        conn.execute("DELETE FROM app_settings WHERE key = ?1", [key])
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    conn.execute(
        "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        rusqlite::params![key, value, now_iso8601()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn now_iso8601() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE app_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL);",
        )
        .unwrap();
        (dir, path)
    }

    #[test]
    fn set_then_get_round_trips() {
        let (_dir, path) = temp_db();
        assert_eq!(get_setting(&path, KEY_DEFAULT_LLM_CREDENTIAL), None);
        set_setting(&path, KEY_DEFAULT_LLM_CREDENTIAL, "cred-1").unwrap();
        assert_eq!(
            get_setting(&path, KEY_DEFAULT_LLM_CREDENTIAL).as_deref(),
            Some("cred-1")
        );
    }

    #[test]
    fn overwrite_and_clear() {
        let (_dir, path) = temp_db();
        set_setting(&path, KEY_DEFAULT_LLM_CREDENTIAL, "cred-1").unwrap();
        set_setting(&path, KEY_DEFAULT_LLM_CREDENTIAL, "cred-2").unwrap();
        assert_eq!(
            get_setting(&path, KEY_DEFAULT_LLM_CREDENTIAL).as_deref(),
            Some("cred-2")
        );
        // 空串 = 清除，读回 None（调用方据此回退到会话自带凭据）
        set_setting(&path, KEY_DEFAULT_LLM_CREDENTIAL, "").unwrap();
        assert_eq!(get_setting(&path, KEY_DEFAULT_LLM_CREDENTIAL), None);
    }

    #[test]
    fn missing_table_is_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.db");
        assert_eq!(get_setting(&path, "any"), None);
        // 写失败要报错（调用方需要知道没存上），但不 panic
        assert!(set_setting(&path, "any", "v").is_err());
    }
}
