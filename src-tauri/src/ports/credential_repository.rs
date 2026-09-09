use thiserror::Error;

use crate::{
    domain::credentials::{CredentialDraft, CredentialRecord},
    ports::persistence::PersistenceError,
};

#[derive(Debug, Error)]
pub enum CredentialRepositoryError {
    #[error("credential not found: {0}")]
    NotFound(String),
    #[error("credential keychain operation failed: {0}")]
    Keychain(String),
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

pub trait CredentialRepository: Send {
    fn list(&mut self) -> Result<Vec<CredentialRecord>, CredentialRepositoryError>;
    fn get(&mut self, id: &str) -> Result<Option<CredentialRecord>, CredentialRepositoryError>;
    fn create(
        &mut self,
        draft: CredentialDraft,
        secret: String,
    ) -> Result<CredentialRecord, CredentialRepositoryError>;
    fn update(
        &mut self,
        id: &str,
        draft: CredentialDraft,
        secret: Option<String>,
    ) -> Result<CredentialRecord, CredentialRepositoryError>;
    fn delete(&mut self, id: &str) -> Result<(), CredentialRepositoryError>;
    /// 读取密钥明文。仅在发起 Provider 请求时调用。
    #[allow(dead_code)]
    fn get_secret(&mut self, credential_key: &str) -> Result<String, CredentialRepositoryError>;
}
