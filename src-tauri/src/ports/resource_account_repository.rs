//! 资源账号仓库端口。
//!
//! 管理 resource_accounts 表的 CRUD 操作，与 credential_repository（管理 API Key）互补。

use thiserror::Error;

use crate::ports::persistence::PersistenceError;
use crate::ports::resource_connector::ResourceAccountRecord;

#[derive(Debug, Error)]
pub enum ResourceAccountRepositoryError {
    #[error("resource account not found: {0}")]
    NotFound(String),
    #[error("keychain operation failed: {0}")]
    Keychain(String),
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

/// 创建资源账号的输入草稿。
#[derive(Debug, Clone)]
pub struct ResourceAccountDraft {
    pub provider_id: String,
    pub account_type: String,
    pub display_name: String,
    pub base_url: String,
    /// 额外元数据 JSON。
    pub extra_json: String,
}

/// 更新资源账号的输入草稿。
#[derive(Debug, Clone)]
pub struct ResourceAccountUpdate {
    pub display_name: String,
    pub base_url: String,
    pub extra_json: String,
}

pub trait ResourceAccountRepository: Send {
    /// 列出所有未删除的资源账号。
    fn list(&mut self) -> Result<Vec<ResourceAccountRecord>, ResourceAccountRepositoryError>;

    /// 按 ID 获取单个账号。
    fn get(
        &mut self,
        id: &str,
    ) -> Result<Option<ResourceAccountRecord>, ResourceAccountRepositoryError>;

    /// 按 provider_id 列出可用账号（enabled + active + 未删除）。
    fn list_by_provider(
        &mut self,
        provider_id: &str,
    ) -> Result<Vec<ResourceAccountRecord>, ResourceAccountRepositoryError>;

    /// 创建资源账号，同时将 session 密钥写入 OS Keychain。
    fn create(
        &mut self,
        draft: ResourceAccountDraft,
        session_secret: String,
    ) -> Result<ResourceAccountRecord, ResourceAccountRepositoryError>;

    /// 更新账号元信息（不含 session）。
    fn update(
        &mut self,
        id: &str,
        update: ResourceAccountUpdate,
    ) -> Result<ResourceAccountRecord, ResourceAccountRepositoryError>;

    /// 更新账号 session 密钥（重新登录时调用）。
    fn update_session(
        &mut self,
        id: &str,
        session_secret: String,
    ) -> Result<(), ResourceAccountRepositoryError>;

    /// 更新账号状态（健康检查结果回写）。
    fn update_status(
        &mut self,
        id: &str,
        status: &str,
    ) -> Result<(), ResourceAccountRepositoryError>;

    /// 切换启用/禁用。
    fn set_enabled(
        &mut self,
        id: &str,
        enabled: bool,
    ) -> Result<(), ResourceAccountRepositoryError>;

    /// 软删除。
    fn delete(&mut self, id: &str) -> Result<(), ResourceAccountRepositoryError>;

    /// 读取 session 密钥明文（仅在发起请求时调用）。
    fn get_session_secret(
        &mut self,
        credential_key: &str,
    ) -> Result<String, ResourceAccountRepositoryError>;
}
