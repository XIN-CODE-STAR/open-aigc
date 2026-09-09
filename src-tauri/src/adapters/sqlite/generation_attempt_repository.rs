#![allow(dead_code)]
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::domain::generation::{AttemptStatus, GenerationAttemptDraft, GenerationAttemptRecord};
use crate::ports::{
    generation_attempt_repository::{AttemptRepositoryError, GenerationAttemptRepository},
    persistence::PersistenceError,
};

pub struct SqliteGenerationAttemptRepository {
    connection: rusqlite::Connection,
}

impl SqliteGenerationAttemptRepository {
    pub fn open(database_path: impl AsRef<std::path::Path>) -> Result<Self, PersistenceError> {
        let connection = crate::adapters::sqlite::database::open_database(database_path.as_ref())?;
        Ok(Self { connection })
    }
}

fn now() -> Result<String, PersistenceError> {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| PersistenceError::new("format timestamp", e))
}

/// SELECT 列列表（与 map_attempt 索引对应）。
const ATTEMPT_COLUMNS: &str = "id,task_id,credential_id,capability,request_snapshot_json,remote_job_id,status,progress,error_code,error_message,result_asset_id,provider_id,consecutive_failures,last_poll_at,started_at,finished_at,created_at,updated_at";

impl GenerationAttemptRepository for SqliteGenerationAttemptRepository {
    fn create(
        &mut self,
        draft: GenerationAttemptDraft,
    ) -> Result<GenerationAttemptRecord, AttemptRepositoryError> {
        let id = Uuid::new_v4().to_string();
        let ts = now()?;
        self.connection.execute(
            "INSERT INTO generation_attempts (id, task_id, credential_id, capability, request_snapshot_json, provider_id, status, progress, consecutive_failures, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,'pending',0,0,?7,?7)",
            params![id, draft.task_id, draft.credential_id, draft.capability, draft.request_snapshot_json, draft.provider_id, ts],
        ).map_err(|e| PersistenceError::new("insert attempt", e))?;
        Ok(GenerationAttemptRecord {
            id,
            task_id: draft.task_id,
            credential_id: draft.credential_id,
            capability: draft.capability,
            request_snapshot_json: draft.request_snapshot_json,
            remote_job_id: None,
            status: AttemptStatus::Pending,
            progress: 0,
            error_code: None,
            error_message: None,
            result_asset_id: None,
            provider_id: draft.provider_id,
            consecutive_failures: 0,
            last_poll_at: None,
            started_at: None,
            finished_at: None,
            created_at: ts.clone(),
            updated_at: ts,
        })
    }

    fn get(&mut self, id: &str) -> Result<Option<GenerationAttemptRecord>, AttemptRepositoryError> {
        self.connection
            .query_row(
                &format!("SELECT {ATTEMPT_COLUMNS} FROM generation_attempts WHERE id=?1"),
                params![id],
                map_attempt,
            )
            .optional()
            .map_err(|e| PersistenceError::new("get attempt", e).into())
    }

    fn list_by_task(
        &mut self,
        task_id: &str,
    ) -> Result<Vec<GenerationAttemptRecord>, AttemptRepositoryError> {
        let mut stmt = self.connection.prepare(
            &format!("SELECT {ATTEMPT_COLUMNS} FROM generation_attempts WHERE task_id=?1 ORDER BY created_at")
        ).map_err(|e| PersistenceError::new("prepare list attempts", e))?;
        let rows = stmt
            .query_map(params![task_id], map_attempt)
            .map_err(|e| PersistenceError::new("query attempts", e))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    fn update_status(
        &mut self,
        id: &str,
        status: AttemptStatus,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<GenerationAttemptRecord, AttemptRepositoryError> {
        let ts = now()?;
        let finished_at = if status.is_terminal() {
            Some(ts.as_str())
        } else {
            None
        };
        self.connection.execute(
            "UPDATE generation_attempts SET status=?1, error_code=?2, error_message=?3, finished_at=?4, updated_at=?5 WHERE id=?6",
            params![status.as_str(), error_code, error_message, finished_at, ts, id],
        ).map_err(|e| PersistenceError::new("update attempt status", e))?;
        self.get(id)?
            .ok_or(AttemptRepositoryError::NotFound(id.into()))
    }

    fn set_remote_job_id(
        &mut self,
        id: &str,
        remote_job_id: &str,
    ) -> Result<(), AttemptRepositoryError> {
        let ts = now()?;
        self.connection
            .execute(
                "UPDATE generation_attempts SET remote_job_id=?1, updated_at=?2 WHERE id=?3",
                params![remote_job_id, ts, id],
            )
            .map_err(|e| PersistenceError::new("set remote job id", e))?;
        Ok(())
    }

    fn update_progress(&mut self, id: &str, progress: u8) -> Result<(), AttemptRepositoryError> {
        let ts = now()?;
        self.connection
            .execute(
                "UPDATE generation_attempts SET progress=?1, updated_at=?2 WHERE id=?3",
                params![progress as i32, ts, id],
            )
            .map_err(|e| PersistenceError::new("update progress", e))?;
        Ok(())
    }

    fn set_result_asset(&mut self, id: &str, asset_id: &str) -> Result<(), AttemptRepositoryError> {
        let ts = now()?;
        self.connection
            .execute(
                "UPDATE generation_attempts SET result_asset_id=?1, updated_at=?2 WHERE id=?3",
                params![asset_id, ts, id],
            )
            .map_err(|e| PersistenceError::new("set result asset", e))?;
        Ok(())
    }

    fn list_active(&mut self) -> Result<Vec<GenerationAttemptRecord>, AttemptRepositoryError> {
        let mut stmt = self.connection.prepare(
            &format!("SELECT {ATTEMPT_COLUMNS} FROM generation_attempts WHERE status IN ('submitted','polling') ORDER BY created_at")
        ).map_err(|e| PersistenceError::new("prepare list active", e))?;
        let rows = stmt
            .query_map([], map_attempt)
            .map_err(|e| PersistenceError::new("query active attempts", e))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    fn increment_failures(&mut self, id: &str) -> Result<(), AttemptRepositoryError> {
        let ts = now()?;
        self.connection.execute(
            "UPDATE generation_attempts SET consecutive_failures = consecutive_failures + 1, last_poll_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![ts, id],
        ).map_err(|e| PersistenceError::new("increment failures", e))?;
        Ok(())
    }

    fn reset_failures(&mut self, id: &str) -> Result<(), AttemptRepositoryError> {
        let ts = now()?;
        self.connection.execute(
            "UPDATE generation_attempts SET consecutive_failures = 0, last_poll_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![ts, id],
        ).map_err(|e| PersistenceError::new("reset failures", e))?;
        Ok(())
    }

    fn set_provider_id(
        &mut self,
        id: &str,
        provider_id: &str,
    ) -> Result<(), AttemptRepositoryError> {
        let ts = now()?;
        self.connection
            .execute(
                "UPDATE generation_attempts SET provider_id = ?1, updated_at = ?2 WHERE id = ?3",
                params![provider_id, ts, id],
            )
            .map_err(|e| PersistenceError::new("set provider id", e))?;
        Ok(())
    }

    fn update_last_poll(&mut self, id: &str) -> Result<(), AttemptRepositoryError> {
        let ts = now()?;
        self.connection
            .execute(
                "UPDATE generation_attempts SET last_poll_at = ?1, updated_at = ?1 WHERE id = ?2",
                params![ts, id],
            )
            .map_err(|e| PersistenceError::new("update last poll", e))?;
        Ok(())
    }
}

fn map_attempt(row: &rusqlite::Row<'_>) -> rusqlite::Result<GenerationAttemptRecord> {
    let status_str: String = row.get(6)?;
    let status = AttemptStatus::parse(&status_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(GenerationAttemptRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        credential_id: row.get(2)?,
        capability: row.get(3)?,
        request_snapshot_json: row.get(4)?,
        remote_job_id: row.get(5)?,
        status,
        progress: row.get::<_, i32>(7)? as u8,
        error_code: row.get(8)?,
        error_message: row.get(9)?,
        result_asset_id: row.get(10)?,
        provider_id: row.get(11)?,
        consecutive_failures: row.get::<_, i32>(12)? as u32,
        last_poll_at: row.get(13)?,
        started_at: row.get(14)?,
        finished_at: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}
