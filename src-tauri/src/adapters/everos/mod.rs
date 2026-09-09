//! EverOS 记忆服务适配器。
//!
//! 通过 HTTP API 与本地 EverOS sidecar 进程通信，
//! 提供长期记忆存储和 RAG 检索能力。

pub mod client;
pub mod process_manager;

pub use client::EverosClient;
pub use process_manager::EverosProcessManager;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EverosError {
    #[error("everos process error: {0}")]
    Process(String),
    #[error("everos network error: {0}")]
    Network(String),
    #[error("everos api error: {0}")]
    Api(String),
    #[error("everos not available")]
    NotAvailable,
}
