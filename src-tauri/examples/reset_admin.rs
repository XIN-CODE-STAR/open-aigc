// 一次性工具：把现有 aigc-studio.sqlite3 里的 admin 账户重置为 123/123。
// 用法：cargo run --example reset_admin
use rusqlite::Connection;

#[derive(Debug)]
struct HashError(String);
impl std::fmt::Display for HashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for HashError {}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let username = std::env::args().nth(1).unwrap_or_else(|| "123".to_string());
    let password = std::env::args().nth(2).unwrap_or_else(|| "123".to_string());
    // argon2id hash of `password`，salt 固定为一次性（不存储到 DB，仅供本次）。
    let hash = {
        use argon2::password_hash::{PasswordHasher, SaltString};
        use argon2::{Algorithm, Argon2, Params, Version};
        use rand::rngs::OsRng;
        let salt = SaltString::generate(&mut OsRng);
        Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default())
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| HashError(e.to_string()))?
            .to_string()
    };

    let db_path = std::env::var("AIGC_DB").unwrap_or_else(|_| {
        let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
        format!(
            "{}\\com.aigcstudio.desktop\\workspace\\aigc-studio.sqlite3",
            local
        )
    });
    println!("DB: {db_path}");

    let conn = Connection::open(&db_path)?;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| format!("format rfc3339: {e}"))?;
    let tx = conn.unchecked_transaction()?;

    // 删除旧的 admin 记录（如有）
    tx.execute(
        "DELETE FROM app_settings WHERE key IN ('admin_username', 'admin_password_hash')",
        [],
    )?;

    // 写入新记录
    tx.execute(
        "INSERT INTO app_settings (key, value, updated_at) VALUES ('admin_username', ?1, ?2)",
        rusqlite::params![username, now],
    )?;
    tx.execute(
        "INSERT INTO app_settings (key, value, updated_at) VALUES ('admin_password_hash', ?1, ?2)",
        rusqlite::params![hash, now],
    )?;

    tx.commit()?;

    // 验证回读
    let mut stmt = conn
        .prepare("SELECT key, substr(value, 1, 40) FROM app_settings WHERE key LIKE 'admin_%'")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (k, v) in rows {
        println!("{k} = {v}…");
    }
    Ok(())
}
