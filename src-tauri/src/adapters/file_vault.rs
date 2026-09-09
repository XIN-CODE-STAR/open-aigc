//! 本地加密文件保险库 — 替代 Windows Credential Manager (keyring crate)。
//!
//! 设计：
//! - 主密钥：首次使用时生成 32 字节随机密钥，存于 `{vault_dir}/master.key`
//! - 密文存储：`{vault_dir}/secrets.json`，JSON 对象 `{ key: base64(nonce‖ciphertext‖tag) }`
//! - 加密算法：AES-256-GCM（认证加密，防篡改）
//!
//! 安全性：主密钥文件与密文文件分离存储；即使 secrets.json 泄露，
//! 没有 master.key 也无法解密。对于桌面教育工具场景足够。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use rand::RngCore;

const MASTER_KEY_FILE: &str = "master.key";
const SECRETS_FILE: &str = "secrets.json";
const NONCE_LEN: usize = 12; // AES-GCM 标准 96-bit nonce
const MASTER_KEY_LEN: usize = 32; // AES-256

/// 文件保险库错误。
#[derive(Debug, thiserror::Error)]
pub enum FileVaultError {
    #[error("vault I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("vault serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("vault encryption error: {0}")]
    Encryption(String),
    #[error("vault base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("secret not found: {0}")]
    NotFound(String),
}

/// 基于 AES-256-GCM 加密文件的密钥存储。
pub struct FileVault {
    vault_dir: PathBuf,
    master_key: [u8; MASTER_KEY_LEN],
}

impl FileVault {
    /// 打开（或初始化）位于 `vault_dir` 的保险库。
    pub fn open(vault_dir: impl AsRef<Path>) -> Result<Self, FileVaultError> {
        let vault_dir = vault_dir.as_ref().to_path_buf();
        fs::create_dir_all(&vault_dir)?;

        let master_key = Self::load_or_create_master_key(&vault_dir)?;
        Ok(Self {
            vault_dir,
            master_key,
        })
    }

    /// 从数据库路径推导 vault 目录：`{db_parent}/.vault/`
    pub fn from_database_path(database_path: &Path) -> Result<Self, FileVaultError> {
        let parent = database_path.parent().unwrap_or_else(|| Path::new("."));
        Self::open(parent.join(".vault"))
    }

    /// 存储密钥（覆盖已有值）。
    pub fn set_secret(&self, key: &str, secret: &str) -> Result<(), FileVaultError> {
        let mut secrets = self.load_secrets()?;
        let encrypted = self.encrypt(secret.as_bytes())?;
        secrets.insert(key.to_owned(), encrypted);
        self.save_secrets(&secrets)
    }

    /// 读取密钥。
    pub fn get_secret(&self, key: &str) -> Result<String, FileVaultError> {
        let secrets = self.load_secrets()?;
        let encoded = secrets
            .get(key)
            .ok_or_else(|| FileVaultError::NotFound(key.to_owned()))?;
        let plaintext = self.decrypt(encoded)?;
        String::from_utf8(plaintext)
            .map_err(|e| FileVaultError::Encryption(format!("invalid UTF-8 in secret: {e}")))
    }

    /// 删除密钥（不存在时静默成功）。
    pub fn delete_secret(&self, key: &str) -> Result<(), FileVaultError> {
        let mut secrets = self.load_secrets()?;
        secrets.remove(key);
        self.save_secrets(&secrets)
    }

    // ─── 内部实现 ───────────────────────────────────────────────

    fn load_or_create_master_key(vault_dir: &Path) -> Result<[u8; MASTER_KEY_LEN], FileVaultError> {
        let key_path = vault_dir.join(MASTER_KEY_FILE);
        if key_path.exists() {
            let bytes = fs::read(&key_path)?;
            if bytes.len() != MASTER_KEY_LEN {
                return Err(FileVaultError::Encryption(format!(
                    "master key length mismatch: expected {MASTER_KEY_LEN}, got {}",
                    bytes.len()
                )));
            }
            let mut key = [0u8; MASTER_KEY_LEN];
            key.copy_from_slice(&bytes);
            Ok(key)
        } else {
            let mut key = [0u8; MASTER_KEY_LEN];
            rand::thread_rng().fill_bytes(&mut key);
            fs::write(&key_path, key)?;
            Ok(key)
        }
    }

    fn cipher(&self) -> Aes256Gcm {
        Aes256Gcm::new_from_slice(&self.master_key).expect("master key length is always 32 bytes")
    }

    fn encrypt(&self, plaintext: &[u8]) -> Result<String, FileVaultError> {
        let cipher = self.cipher();
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| FileVaultError::Encryption(e.to_string()))?;

        // 格式: nonce(12) || ciphertext+tag
        let mut combined = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        combined.extend_from_slice(&nonce_bytes);
        combined.extend_from_slice(&ciphertext);
        Ok(BASE64.encode(&combined))
    }

    fn decrypt(&self, encoded: &str) -> Result<Vec<u8>, FileVaultError> {
        let combined = BASE64.decode(encoded)?;
        if combined.len() < NONCE_LEN + 16 {
            return Err(FileVaultError::Encryption(
                "ciphertext too short".to_owned(),
            ));
        }
        let (nonce_bytes, ciphertext) = combined.split_at(NONCE_LEN);
        let nonce = Nonce::from_slice(nonce_bytes);
        let cipher = self.cipher();

        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| FileVaultError::Encryption(format!("decryption failed: {e}")))
    }

    fn secrets_path(&self) -> PathBuf {
        self.vault_dir.join(SECRETS_FILE)
    }

    fn load_secrets(&self) -> Result<HashMap<String, String>, FileVaultError> {
        let path = self.secrets_path();
        if !path.exists() {
            return Ok(HashMap::new());
        }
        let content = fs::read_to_string(&path)?;
        let map: HashMap<String, String> = serde_json::from_str(&content)?;
        Ok(map)
    }

    fn save_secrets(&self, secrets: &HashMap<String, String>) -> Result<(), FileVaultError> {
        let content = serde_json::to_string_pretty(secrets)?;
        fs::write(self.secrets_path(), content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn round_trip_secret() {
        let dir = tempdir().unwrap();
        let vault = FileVault::open(dir.path()).unwrap();
        vault.set_secret("credential:abc", "sk-test-123").unwrap();
        let recovered = vault.get_secret("credential:abc").unwrap();
        assert_eq!(recovered, "sk-test-123");
    }

    #[test]
    fn overwrite_secret() {
        let dir = tempdir().unwrap();
        let vault = FileVault::open(dir.path()).unwrap();
        vault.set_secret("k", "v1").unwrap();
        vault.set_secret("k", "v2").unwrap();
        assert_eq!(vault.get_secret("k").unwrap(), "v2");
    }

    #[test]
    fn delete_secret() {
        let dir = tempdir().unwrap();
        let vault = FileVault::open(dir.path()).unwrap();
        vault.set_secret("k", "v").unwrap();
        vault.delete_secret("k").unwrap();
        assert!(vault.get_secret("k").is_err());
    }

    #[test]
    fn persists_across_instances() {
        let dir = tempdir().unwrap();
        {
            let vault = FileVault::open(dir.path()).unwrap();
            vault.set_secret("k", "persisted").unwrap();
        }
        let vault2 = FileVault::open(dir.path()).unwrap();
        assert_eq!(vault2.get_secret("k").unwrap(), "persisted");
    }

    #[test]
    fn missing_key_returns_not_found() {
        let dir = tempdir().unwrap();
        let vault = FileVault::open(dir.path()).unwrap();
        let err = vault.get_secret("nope").unwrap_err();
        assert!(matches!(err, FileVaultError::NotFound(_)));
    }
}
