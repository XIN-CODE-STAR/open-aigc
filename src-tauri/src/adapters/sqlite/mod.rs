use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::ports::persistence::PersistenceError;

mod agent_repository;
pub mod analysis_repository;
mod asset_import;
mod asset_integrity;
pub mod asset_repository;
pub mod backup_repository;
pub mod canvas_repository;
pub mod creative_memory_repository;
pub mod creative_state_repository;
pub mod credential_repository;
pub mod database;
pub mod edit_repository;
pub mod generation_attempt_repository;
pub mod generation_repository;
pub(crate) mod managed_storage;
pub mod manga_repository;
pub mod memory_canvas_repository;
mod migration_backup;
pub mod plan_repository;
pub mod resource_account_repository;
pub mod resource_repository;
pub mod review_repository;
pub mod semantic_repository;
pub mod workflow_execution_repository;
pub mod workspace_repository;

pub use agent_repository::SqliteAgentRepository;
pub use analysis_repository::SqliteAnalysisJobRepository;
pub use manga_repository::SqliteMangaRepository;
pub use plan_repository::SqlitePlanRepository;

/// 生成当前 UTC 时间的 RFC3339 字符串，供 SQLite 仓库写入 created_at/updated_at。
pub fn now_rfc3339() -> Result<String, PersistenceError> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| PersistenceError::new("format timestamp", error))
}

/// 将搜索字符串转换为 SQL LIKE 模式：前后加 `%`，转义 `\`、`%`、`_`。
pub fn like_pattern(value: &str) -> String {
    let escaped = value
        .replace('\\', r"\\")
        .replace('%', r"\%")
        .replace('_', r"\_");
    format!("%{escaped}%")
}
