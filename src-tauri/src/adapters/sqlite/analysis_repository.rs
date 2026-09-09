use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use crate::application::error::AppError;
use crate::domain::analysis::{AnalysisJob, AnalysisJobDraft, AnalysisJobStatus, AnalysisJobType};
use crate::ports::analysis_repository::AnalysisJobRepository;
use crate::ports::persistence::PersistenceError;
use crate::ports::reloadable::Reloadable;

/// SQLite implementation of AnalysisJobRepository.
pub struct SqliteAnalysisJobRepository {
    connection: Mutex<Connection>,
}

impl SqliteAnalysisJobRepository {
    pub fn open(connection: Connection) -> Self {
        Self {
            connection: Mutex::new(connection),
        }
    }
}

impl Reloadable for SqliteAnalysisJobRepository {
    fn reload(&self, _database_path: &Path) -> Result<(), AppError> {
        Ok(())
    }
}

// ── Mapping helper ──

fn map_job(row: &rusqlite::Row) -> rusqlite::Result<AnalysisJob> {
    let job_type_str: String = row.get(1)?;
    let status_str: String = row.get(2)?;

    let job_type = AnalysisJobType::parse(&job_type_str).unwrap_or(AnalysisJobType::Hash);
    let status = AnalysisJobStatus::parse(&status_str).unwrap_or(AnalysisJobStatus::Queued);

    Ok(AnalysisJob {
        id: row.get(0)?,
        job_type,
        status,
        adapter_id: row.get(3)?,
        priority: row.get(4)?,
        created_at: row.get(5)?,
        started_at: row.get(6)?,
        completed_at: row.get(7)?,
        progress: row.get(8)?,
        error_message: row.get(9)?,
        result_json: row.get(10)?,
        input_asset_id: row.get(11)?,
        input_job_id: row.get(12)?,
    })
}

impl AnalysisJobRepository for SqliteAnalysisJobRepository {
    fn create_job(&self, draft: &AnalysisJobDraft) -> Result<AnalysisJob, AppError> {
        draft
            .validate()
            .map_err(|e| AppError::Workflow(format!("invalid job draft: {e}")))?;

        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;

        let mut job = AnalysisJob::new(
            uuid::Uuid::new_v4().to_string(),
            draft.job_type.clone(),
            draft.input_asset_id.clone(),
            draft.input_job_id.clone(),
        );
        if let Some(priority) = draft.priority {
            job.priority = priority;
        }

        connection
            .execute(
                "INSERT INTO analysis_jobs (id, job_type, status, adapter_id, priority, created_at, started_at, completed_at, progress, error_message, result_json, input_asset_id, input_job_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    job.id,
                    job.job_type.as_str(),
                    job.status.as_str(),
                    job.adapter_id,
                    job.priority,
                    job.created_at,
                    job.started_at,
                    job.completed_at,
                    job.progress,
                    job.error_message,
                    job.result_json,
                    job.input_asset_id,
                    job.input_job_id,
                ],
            )
            .map_err(|e| PersistenceError::new("create_job", e))?;

        Ok(job)
    }

    fn get_job(&self, job_id: &str) -> Result<Option<AnalysisJob>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let result = connection
            .query_row(
                "SELECT id, job_type, status, adapter_id, priority, created_at, started_at, completed_at, progress, error_message, result_json, input_asset_id, input_job_id FROM analysis_jobs WHERE id = ?1",
                params![job_id],
                map_job,
            )
            .optional()
            .map_err(|e| PersistenceError::new("get_job", e))?;

        Ok(result)
    }

    fn update_job(&self, job: &AnalysisJob) -> Result<(), AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;

        connection
            .execute(
                "UPDATE analysis_jobs SET status = ?1, adapter_id = ?2, priority = ?3, started_at = ?4, completed_at = ?5, progress = ?6, error_message = ?7, result_json = ?8 WHERE id = ?9",
                params![
                    job.status.as_str(),
                    job.adapter_id,
                    job.priority,
                    job.started_at,
                    job.completed_at,
                    job.progress,
                    job.error_message,
                    job.result_json,
                    job.id,
                ],
            )
            .map_err(|e| PersistenceError::new("update_job", e))?;

        Ok(())
    }

    fn delete_job(&self, job_id: &str) -> Result<(), AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;

        connection
            .execute("DELETE FROM analysis_jobs WHERE id = ?1", params![job_id])
            .map_err(|e| PersistenceError::new("delete_job", e))?;

        Ok(())
    }

    fn get_jobs_for_asset(&self, asset_id: &str) -> Result<Vec<AnalysisJob>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let mut stmt = connection
            .prepare(
                "SELECT id, job_type, status, adapter_id, priority, created_at, started_at, completed_at, progress, error_message, result_json, input_asset_id, input_job_id FROM analysis_jobs WHERE input_asset_id = ?1 ORDER BY created_at DESC",
            )
            .map_err(|e| PersistenceError::new("get_jobs_for_asset prepare", e))?;

        let rows = stmt
            .query_map(params![asset_id], map_job)
            .map_err(|e| PersistenceError::new("get_jobs_for_asset query", e))?;

        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(row.map_err(|e| PersistenceError::new("get_jobs_for_asset row", e))?);
        }

        Ok(jobs)
    }

    fn get_jobs_waiting_for(&self, job_id: &str) -> Result<Vec<AnalysisJob>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let mut stmt = connection
            .prepare(
                "SELECT id, job_type, status, adapter_id, priority, created_at, started_at, completed_at, progress, error_message, result_json, input_asset_id, input_job_id FROM analysis_jobs WHERE input_job_id = ?1 AND status = 'Queued'",
            )
            .map_err(|e| PersistenceError::new("get_jobs_waiting_for prepare", e))?;

        let rows = stmt
            .query_map(params![job_id], map_job)
            .map_err(|e| PersistenceError::new("get_jobs_waiting_for query", e))?;

        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(row.map_err(|e| PersistenceError::new("get_jobs_waiting_for row", e))?);
        }

        Ok(jobs)
    }

    fn get_queued_jobs(&self, limit: usize) -> Result<Vec<AnalysisJob>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let mut stmt = connection
            .prepare(
                "SELECT id, job_type, status, adapter_id, priority, created_at, started_at, completed_at, progress, error_message, result_json, input_asset_id, input_job_id FROM analysis_jobs WHERE status = 'Queued' ORDER BY priority DESC, created_at ASC LIMIT ?1",
            )
            .map_err(|e| PersistenceError::new("get_queued_jobs prepare", e))?;

        let rows = stmt
            .query_map(params![limit as i64], map_job)
            .map_err(|e| PersistenceError::new("get_queued_jobs query", e))?;

        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(row.map_err(|e| PersistenceError::new("get_queued_jobs row", e))?);
        }

        Ok(jobs)
    }

    fn get_running_jobs(&self) -> Result<Vec<AnalysisJob>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let mut stmt = connection
            .prepare(
                "SELECT id, job_type, status, adapter_id, priority, created_at, started_at, completed_at, progress, error_message, result_json, input_asset_id, input_job_id FROM analysis_jobs WHERE status = 'Running'",
            )
            .map_err(|e| PersistenceError::new("get_running_jobs prepare", e))?;

        let rows = stmt
            .query_map([], map_job)
            .map_err(|e| PersistenceError::new("get_running_jobs query", e))?;

        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(row.map_err(|e| PersistenceError::new("get_running_jobs row", e))?);
        }

        Ok(jobs)
    }

    fn get_terminal_jobs_for_asset(&self, asset_id: &str) -> Result<Vec<AnalysisJob>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let mut stmt = connection
            .prepare(
                "SELECT id, job_type, status, adapter_id, priority, created_at, started_at, completed_at, progress, error_message, result_json, input_asset_id, input_job_id FROM analysis_jobs WHERE input_asset_id = ?1 AND status IN ('Completed', 'Failed') ORDER BY completed_at DESC",
            )
            .map_err(|e| PersistenceError::new("get_terminal_jobs_for_asset prepare", e))?;

        let rows = stmt
            .query_map(params![asset_id], map_job)
            .map_err(|e| PersistenceError::new("get_terminal_jobs_for_asset query", e))?;

        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(
                row.map_err(|e| PersistenceError::new("get_terminal_jobs_for_asset row", e))?,
            );
        }

        Ok(jobs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE analysis_jobs (
                id TEXT PRIMARY KEY,
                job_type TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'Queued',
                adapter_id TEXT,
                priority INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                started_at TEXT,
                completed_at TEXT,
                progress REAL,
                error_message TEXT,
                result_json TEXT,
                input_asset_id TEXT,
                input_job_id TEXT
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn create_and_get_job() {
        let repo = SqliteAnalysisJobRepository::open(setup_db());
        let draft = AnalysisJobDraft {
            job_type: AnalysisJobType::Caption,
            input_asset_id: Some("art-1".to_owned()),
            input_job_id: None,
            priority: None,
        };

        let job = repo.create_job(&draft).unwrap();
        assert_eq!(job.job_type, AnalysisJobType::Caption);
        assert_eq!(job.status, AnalysisJobStatus::Queued);
        assert_eq!(job.input_asset_id, Some("art-1".to_owned()));

        let got = repo.get_job(&job.id).unwrap().unwrap();
        assert_eq!(got.id, job.id);
        assert_eq!(got.job_type, AnalysisJobType::Caption);
    }

    #[test]
    fn update_job_status() {
        let repo = SqliteAnalysisJobRepository::open(setup_db());
        let draft = AnalysisJobDraft {
            job_type: AnalysisJobType::VisionAnalysis,
            input_asset_id: Some("art-1".to_owned()),
            input_job_id: None,
            priority: None,
        };

        let mut job = repo.create_job(&draft).unwrap();
        job.start("dashscope".to_owned());
        repo.update_job(&job).unwrap();

        let got = repo.get_job(&job.id).unwrap().unwrap();
        assert_eq!(got.status, AnalysisJobStatus::Running);
        assert_eq!(got.adapter_id, Some("dashscope".to_owned()));

        job.complete(r#"{"caption": "test"}"#.to_owned());
        repo.update_job(&job).unwrap();

        let got = repo.get_job(&job.id).unwrap().unwrap();
        assert_eq!(got.status, AnalysisJobStatus::Completed);
        assert!(got.result_json.is_some());
    }

    #[test]
    fn get_queued_jobs_respects_priority() {
        let repo = SqliteAnalysisJobRepository::open(setup_db());

        let draft1 = AnalysisJobDraft {
            job_type: AnalysisJobType::Caption,
            input_asset_id: Some("art-1".to_owned()),
            input_job_id: None,
            priority: Some(0),
        };
        let draft2 = AnalysisJobDraft {
            job_type: AnalysisJobType::Caption,
            input_asset_id: Some("art-2".to_owned()),
            input_job_id: None,
            priority: Some(10),
        };

        repo.create_job(&draft1).unwrap();
        repo.create_job(&draft2).unwrap();

        let queued = repo.get_queued_jobs(10).unwrap();
        assert_eq!(queued.len(), 2);
        assert_eq!(queued[0].priority, 10); // Higher priority first
    }
}
