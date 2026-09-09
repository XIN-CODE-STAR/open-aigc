#![allow(dead_code)]
//! SQLite 实现：WorkflowExecutionRepository。
//!
//! 三表：workflow_runs / execution_steps / generation_submissions。
//! 建表语句使用 CREATE TABLE IF NOT EXISTS，幂等安全。

use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::domain::execution_persistence::{
    ExecutionStepDraft, ExecutionStepRecord, GenerationSubmissionDraft, GenerationSubmissionRecord,
    WorkflowRunDraft, WorkflowRunRecord,
};
use crate::ports::{
    persistence::PersistenceError,
    workflow_execution_repository::{
        WorkflowExecutionRepository, WorkflowExecutionRepositoryError,
    },
};

pub struct SqliteWorkflowExecutionRepository {
    connection: rusqlite::Connection,
}

impl SqliteWorkflowExecutionRepository {
    pub fn open(database_path: impl AsRef<std::path::Path>) -> Result<Self, PersistenceError> {
        let connection = crate::adapters::sqlite::database::open_database(database_path.as_ref())?;
        let repo = Self { connection };
        repo.run_migrations()?;
        Ok(repo)
    }

    fn run_migrations(&self) -> Result<(), PersistenceError> {
        self.connection
            .execute_batch(
                "
CREATE TABLE IF NOT EXISTS workflow_runs (
    id TEXT PRIMARY KEY NOT NULL,
    workspace_id TEXT NOT NULL,
    plan_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'running',
    total_steps INTEGER NOT NULL,
    completed_steps INTEGER NOT NULL DEFAULT 0,
    failed_steps INTEGER NOT NULL DEFAULT 0,
    revision_iteration INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS execution_steps (
    id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL REFERENCES workflow_runs(id),
    step_index INTEGER NOT NULL,
    kind TEXT NOT NULL,
    description TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    output_artifact_id TEXT,
    error TEXT,
    started_at TEXT,
    completed_at TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_execution_steps_run_id ON execution_steps(run_id);

CREATE TABLE IF NOT EXISTS generation_submissions (
    id TEXT PRIMARY KEY NOT NULL,
    step_id TEXT NOT NULL REFERENCES execution_steps(id),
    step_index INTEGER NOT NULL,
    submission_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    model TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'submitted',
    asset_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_generation_submissions_step_id ON generation_submissions(step_id);
CREATE INDEX IF NOT EXISTS idx_generation_submissions_submission_id ON generation_submissions(submission_id);
",
            )
            .map_err(|e| PersistenceError::new("workflow execution migrations", e))?;
        Ok(())
    }
}

fn now() -> Result<String, PersistenceError> {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| PersistenceError::new("format timestamp", e))
}

// ── Column lists ──

const RUN_COLUMNS: &str = "id,workspace_id,plan_id,status,total_steps,completed_steps,failed_steps,revision_iteration,created_at,updated_at,metadata_json";
const STEP_COLUMNS: &str = "id,run_id,step_index,kind,description,status,output_artifact_id,error,started_at,completed_at,metadata_json";
const SUBMISSION_COLUMNS: &str =
    "id,step_id,step_index,submission_id,provider_id,model,status,asset_id,created_at,updated_at";

// ── Row mappers ──

fn map_run(row: &rusqlite::Row) -> rusqlite::Result<WorkflowRunRecord> {
    Ok(WorkflowRunRecord {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        plan_id: row.get(2)?,
        status: row.get(3)?,
        total_steps: row.get(4)?,
        completed_steps: row.get(5)?,
        failed_steps: row.get(6)?,
        revision_iteration: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        metadata_json: row.get(10)?,
    })
}

fn map_step(row: &rusqlite::Row) -> rusqlite::Result<ExecutionStepRecord> {
    Ok(ExecutionStepRecord {
        id: row.get(0)?,
        run_id: row.get(1)?,
        step_index: row.get(2)?,
        kind: row.get(3)?,
        description: row.get(4)?,
        status: row.get(5)?,
        output_artifact_id: row.get(6)?,
        error: row.get(7)?,
        started_at: row.get(8)?,
        completed_at: row.get(9)?,
        metadata_json: row.get(10)?,
    })
}

fn map_submission(row: &rusqlite::Row) -> rusqlite::Result<GenerationSubmissionRecord> {
    Ok(GenerationSubmissionRecord {
        id: row.get(0)?,
        step_id: row.get(1)?,
        step_index: row.get(2)?,
        submission_id: row.get(3)?,
        provider_id: row.get(4)?,
        model: row.get(5)?,
        status: row.get(6)?,
        asset_id: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

// ── Repository implementation ──

impl WorkflowExecutionRepository for SqliteWorkflowExecutionRepository {
    // ── WorkflowRun ──

    fn create_run(
        &mut self,
        draft: WorkflowRunDraft,
    ) -> Result<WorkflowRunRecord, WorkflowExecutionRepositoryError> {
        let id = Uuid::new_v4().to_string();
        let ts = now()?;
        self.connection
            .execute(
                "INSERT INTO workflow_runs (id, workspace_id, plan_id, status, total_steps, created_at, updated_at) VALUES (?1,?2,?3,'running',?4,?5,?5)",
                params![id, draft.workspace_id, draft.plan_id, draft.total_steps, ts],
            )
            .map_err(|e| PersistenceError::new("insert workflow_run", e))?;
        Ok(WorkflowRunRecord {
            id,
            workspace_id: draft.workspace_id,
            plan_id: draft.plan_id,
            status: "running".to_owned(),
            total_steps: draft.total_steps,
            completed_steps: 0,
            failed_steps: 0,
            revision_iteration: 0,
            created_at: ts.clone(),
            updated_at: ts,
            metadata_json: "{}".to_owned(),
        })
    }

    fn update_run_status(
        &mut self,
        id: &str,
        status: &str,
        completed_steps: u32,
        failed_steps: u32,
    ) -> Result<WorkflowRunRecord, WorkflowExecutionRepositoryError> {
        let ts = now()?;
        let rows = self
            .connection
            .execute(
                "UPDATE workflow_runs SET status=?2, completed_steps=?3, failed_steps=?4, updated_at=?5 WHERE id=?1",
                params![id, status, completed_steps, failed_steps, ts],
            )
            .map_err(|e| PersistenceError::new("update workflow_run status", e))?;
        if rows == 0 {
            return Err(WorkflowExecutionRepositoryError::RunNotFound(id.to_owned()));
        }
        self.get_run(id)?
            .ok_or_else(|| WorkflowExecutionRepositoryError::RunNotFound(id.to_owned()))
    }

    fn get_run(
        &mut self,
        id: &str,
    ) -> Result<Option<WorkflowRunRecord>, WorkflowExecutionRepositoryError> {
        self.connection
            .query_row(
                &format!("SELECT {RUN_COLUMNS} FROM workflow_runs WHERE id=?1"),
                params![id],
                map_run,
            )
            .optional()
            .map_err(|e| PersistenceError::new("get workflow_run", e).into())
    }

    fn list_runs_by_workspace(
        &mut self,
        workspace_id: &str,
    ) -> Result<Vec<WorkflowRunRecord>, WorkflowExecutionRepositoryError> {
        let mut stmt = self
            .connection
            .prepare(&format!(
                "SELECT {RUN_COLUMNS} FROM workflow_runs WHERE workspace_id=?1 ORDER BY created_at DESC"
            ))
            .map_err(|e| PersistenceError::new("prepare list runs", e))?;
        let rows = stmt
            .query_map(params![workspace_id], map_run)
            .map_err(|e| PersistenceError::new("list runs", e))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| PersistenceError::new("map run row", e))?);
        }
        Ok(results)
    }

    // ── ExecutionStep ──

    fn create_step(
        &mut self,
        draft: ExecutionStepDraft,
    ) -> Result<ExecutionStepRecord, WorkflowExecutionRepositoryError> {
        let id = Uuid::new_v4().to_string();
        let ts = now()?;
        self.connection
            .execute(
                "INSERT INTO execution_steps (id, run_id, step_index, kind, description, status, started_at) VALUES (?1,?2,?3,?4,?5,'running',?6)",
                params![id, draft.run_id, draft.step_index, draft.kind, draft.description, ts],
            )
            .map_err(|e| PersistenceError::new("insert execution_step", e))?;
        Ok(ExecutionStepRecord {
            id,
            run_id: draft.run_id,
            step_index: draft.step_index,
            kind: draft.kind,
            description: draft.description,
            status: "running".to_owned(),
            output_artifact_id: None,
            error: None,
            started_at: Some(ts),
            completed_at: None,
            metadata_json: "{}".to_owned(),
        })
    }

    fn update_step_status(
        &mut self,
        id: &str,
        status: &str,
        output_artifact_id: Option<&str>,
        error: Option<&str>,
    ) -> Result<ExecutionStepRecord, WorkflowExecutionRepositoryError> {
        let ts = now()?;
        let rows = self
            .connection
            .execute(
                "UPDATE execution_steps SET status=?2, output_artifact_id=?3, error=?4, completed_at=?5 WHERE id=?1",
                params![id, status, output_artifact_id, error, ts],
            )
            .map_err(|e| PersistenceError::new("update execution_step status", e))?;
        if rows == 0 {
            return Err(WorkflowExecutionRepositoryError::StepNotFound(
                id.to_owned(),
            ));
        }
        self.connection
            .query_row(
                &format!("SELECT {STEP_COLUMNS} FROM execution_steps WHERE id=?1"),
                params![id],
                map_step,
            )
            .optional()
            .map_err(|e| PersistenceError::new("get updated step", e))?
            .ok_or_else(|| WorkflowExecutionRepositoryError::StepNotFound(id.to_owned()))
    }

    fn list_steps_by_run(
        &mut self,
        run_id: &str,
    ) -> Result<Vec<ExecutionStepRecord>, WorkflowExecutionRepositoryError> {
        let mut stmt = self
            .connection
            .prepare(&format!(
                "SELECT {STEP_COLUMNS} FROM execution_steps WHERE run_id=?1 ORDER BY step_index"
            ))
            .map_err(|e| PersistenceError::new("prepare list steps", e))?;
        let rows = stmt
            .query_map(params![run_id], map_step)
            .map_err(|e| PersistenceError::new("list steps", e))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| PersistenceError::new("map step row", e))?);
        }
        Ok(results)
    }

    // ── GenerationSubmission ──

    fn create_submission(
        &mut self,
        draft: GenerationSubmissionDraft,
    ) -> Result<GenerationSubmissionRecord, WorkflowExecutionRepositoryError> {
        let id = Uuid::new_v4().to_string();
        let ts = now()?;
        self.connection
            .execute(
                "INSERT INTO generation_submissions (id, step_id, step_index, submission_id, provider_id, model, status, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,'submitted',?7,?7)",
                params![id, draft.step_id, draft.step_index, draft.submission_id, draft.provider_id, draft.model, ts],
            )
            .map_err(|e| PersistenceError::new("insert generation_submission", e))?;
        Ok(GenerationSubmissionRecord {
            id,
            step_id: draft.step_id,
            step_index: draft.step_index,
            submission_id: draft.submission_id,
            provider_id: draft.provider_id,
            model: draft.model,
            status: "submitted".to_owned(),
            asset_id: None,
            created_at: ts.clone(),
            updated_at: ts,
        })
    }

    fn update_submission_status(
        &mut self,
        id: &str,
        status: &str,
        asset_id: Option<&str>,
    ) -> Result<GenerationSubmissionRecord, WorkflowExecutionRepositoryError> {
        let ts = now()?;
        let rows = self
            .connection
            .execute(
                "UPDATE generation_submissions SET status=?2, asset_id=?3, updated_at=?4 WHERE id=?1",
                params![id, status, asset_id, ts],
            )
            .map_err(|e| PersistenceError::new("update submission status", e))?;
        if rows == 0 {
            return Err(WorkflowExecutionRepositoryError::SubmissionNotFound(
                id.to_owned(),
            ));
        }
        self.connection
            .query_row(
                &format!("SELECT {SUBMISSION_COLUMNS} FROM generation_submissions WHERE id=?1"),
                params![id],
                map_submission,
            )
            .optional()
            .map_err(|e| PersistenceError::new("get updated submission", e))?
            .ok_or_else(|| WorkflowExecutionRepositoryError::SubmissionNotFound(id.to_owned()))
    }

    fn list_submissions_by_step(
        &mut self,
        step_id: &str,
    ) -> Result<Vec<GenerationSubmissionRecord>, WorkflowExecutionRepositoryError> {
        let mut stmt = self
            .connection
            .prepare(&format!(
                "SELECT {SUBMISSION_COLUMNS} FROM generation_submissions WHERE step_id=?1 ORDER BY created_at"
            ))
            .map_err(|e| PersistenceError::new("prepare list submissions", e))?;
        let rows = stmt
            .query_map(params![step_id], map_submission)
            .map_err(|e| PersistenceError::new("list submissions", e))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| PersistenceError::new("map submission row", e))?);
        }
        Ok(results)
    }
}
