use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

use crate::{
    domain::generation::{
        GenerationFilter, GenerationResultRecord, GenerationStatus, GenerationTaskDraft,
        GenerationTaskRecord,
    },
    ports::{
        generation_repository::{GenerationRepository, GenerationRepositoryError},
        persistence::PersistenceError,
    },
};

pub struct SqliteGenerationRepository {
    connection: Connection,
}

impl SqliteGenerationRepository {
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        use crate::adapters::sqlite::database::open_database;
        let connection = open_database(database_path.as_ref())?;
        Ok(Self { connection })
    }
}

impl GenerationRepository for SqliteGenerationRepository {
    fn create_task(
        &mut self,
        draft: GenerationTaskDraft,
    ) -> Result<GenerationTaskRecord, GenerationRepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::new("begin generation task transaction", error))?;
        let id = Uuid::new_v4().to_string();
        let now = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|error| PersistenceError::new("format generation task timestamp", error))?;

        transaction
            .execute(
                r#"
                INSERT INTO generation_tasks (
                  id, workspace_id, provider_name, model_name, prompt_text,
                  status, error_message, parameters_json, revision, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, 'pending', NULL, '{}', 1, ?6, ?6)
                "#,
                params![
                    id,
                    draft.workspace_id,
                    draft.provider_name,
                    draft.model_name,
                    draft.prompt_text,
                    now
                ],
            )
            .map_err(|error| PersistenceError::new("insert generation task", error))?;

        transaction
            .commit()
            .map_err(|error| PersistenceError::new("commit generation task", error))?;

        Ok(GenerationTaskRecord {
            id,
            workspace_id: draft.workspace_id,
            provider_name: draft.provider_name,
            model_name: draft.model_name,
            prompt_text: draft.prompt_text,
            status: GenerationStatus::Pending,
            error_message: None,
            progress: 0,
            revision: 1,
            created_at: now.clone(),
            updated_at: now,
            started_at: None,
            completed_at: None,
        })
    }

    fn update_status(
        &mut self,
        task_id: &str,
        status: GenerationStatus,
        error_message: Option<String>,
    ) -> Result<GenerationTaskRecord, GenerationRepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::new("begin generation status transaction", error))?;
        let now = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|error| PersistenceError::new("format generation status timestamp", error))?;

        let started_at: Option<String> = if status == GenerationStatus::Running {
            Some(now.clone())
        } else {
            None
        };
        let completed_at: Option<String> = if matches!(
            status,
            GenerationStatus::Succeeded | GenerationStatus::Failed
        ) {
            Some(now.clone())
        } else {
            None
        };

        let affected = if started_at.is_some() {
            transaction.execute(
                r#"
                UPDATE generation_tasks
                SET status = ?1, updated_at = ?2, started_at = COALESCE(started_at, ?3), error_message = ?4
                WHERE id = ?5
                "#,
                params![status.as_str(), now, started_at, error_message, task_id],
            )
        } else if completed_at.is_some() {
            transaction.execute(
                r#"
                UPDATE generation_tasks
                SET status = ?1, updated_at = ?2, completed_at = ?3, error_message = ?4
                WHERE id = ?5
                "#,
                params![status.as_str(), now, completed_at, error_message, task_id],
            )
        } else {
            transaction.execute(
                r#"
                UPDATE generation_tasks
                SET status = ?1, updated_at = ?2, error_message = ?3
                WHERE id = ?4
                "#,
                params![status.as_str(), now, error_message, task_id],
            )
        }
        .map_err(|error| PersistenceError::new("update generation task status", error))?;

        if affected == 0 {
            return Err(GenerationRepositoryError::TaskNotFound(task_id.to_owned()));
        }

        let record = transaction
            .query_row(
                r#"
                SELECT id, workspace_id, provider_name, model_name, prompt_text,
                       status, error_message, progress, revision, created_at, updated_at,
                       started_at, completed_at
                FROM generation_tasks WHERE id = ?1
                "#,
                params![task_id],
                map_task_record,
            )
            .map_err(|error| PersistenceError::new("read updated generation task", error))?;

        transaction
            .commit()
            .map_err(|error| PersistenceError::new("commit generation status", error))?;

        Ok(record)
    }

    fn get_task(
        &mut self,
        task_id: &str,
    ) -> Result<Option<GenerationTaskRecord>, GenerationRepositoryError> {
        let record = self
            .connection
            .query_row(
                r#"
                SELECT id, workspace_id, provider_name, model_name, prompt_text,
                       status, error_message, progress, revision, created_at, updated_at,
                       started_at, completed_at
                FROM generation_tasks WHERE id = ?1
                "#,
                params![task_id],
                map_task_record,
            )
            .optional()
            .map_err(|error| PersistenceError::new("read generation task", error))?;
        Ok(record)
    }

    fn list_tasks(
        &mut self,
        filter: &GenerationFilter,
    ) -> Result<Vec<GenerationTaskRecord>, GenerationRepositoryError> {
        let mut query = String::from(
            r#"
            SELECT id, workspace_id, provider_name, model_name, prompt_text,
                   status, error_message, progress, revision, created_at, updated_at,
                   started_at, completed_at
            FROM generation_tasks WHERE 1 = 1
            "#,
        );
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let mut param_index = 1;
        if let Some(status) = filter.status {
            query.push_str(&format!(" AND status = ?{param_index}"));
            params_vec.push(Box::new(status.as_str().to_owned()));
            param_index += 1;
        }
        if let Some(provider) = filter.provider_name.as_deref() {
            query.push_str(&format!(" AND provider_name = ?{param_index}"));
            params_vec.push(Box::new(provider.to_owned()));
            param_index += 1;
        }
        query.push_str(&format!(" ORDER BY created_at DESC LIMIT ?{param_index}"));
        params_vec.push(Box::new(filter.limit));

        let mut statement = self
            .connection
            .prepare(&query)
            .map_err(|error| PersistenceError::new("prepare generation list", error))?;
        let rows = statement
            .query_map(
                rusqlite::params_from_iter(params_vec.iter().map(|b| b.as_ref())),
                map_task_record,
            )
            .map_err(|error| PersistenceError::new("query generation list", error))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|error| PersistenceError::new("read generation row", error))?);
        }
        Ok(records)
    }

    fn add_result(
        &mut self,
        task_id: &str,
        asset_id: &str,
    ) -> Result<GenerationResultRecord, GenerationRepositoryError> {
        // 校验 asset 存在。
        let exists: bool = self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM asset_manifest WHERE id = ?1 AND deleted_at IS NULL)",
                params![asset_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|exists| exists == 1)
            .map_err(|error| PersistenceError::new("check asset exists for result", error))?;
        if !exists {
            return Err(GenerationRepositoryError::AssetNotFound(
                asset_id.to_owned(),
            ));
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::new("begin generation result transaction", error))?;
        let id = Uuid::new_v4().to_string();
        let now = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|error| PersistenceError::new("format generation result timestamp", error))?;

        let result = transaction.execute(
            r#"
            INSERT INTO generation_results (id, task_id, asset_id, created_at)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            params![id, task_id, asset_id, now],
        );

        if let Err(error) = result {
            if is_unique_violation(&error) {
                return Err(GenerationRepositoryError::DuplicateResult {
                    task_id: task_id.to_owned(),
                    asset_id: asset_id.to_owned(),
                });
            }
            return Err(GenerationRepositoryError::Persistence(
                PersistenceError::new("insert generation result", error),
            ));
        }

        transaction
            .commit()
            .map_err(|error| PersistenceError::new("commit generation result", error))?;

        Ok(GenerationResultRecord {
            id,
            task_id: task_id.to_owned(),
            asset_id: asset_id.to_owned(),
            created_at: now,
        })
    }

    fn list_results(
        &mut self,
        task_id: &str,
    ) -> Result<Vec<GenerationResultRecord>, GenerationRepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                r#"
                SELECT id, task_id, asset_id, created_at
                FROM generation_results
                WHERE task_id = ?1
                ORDER BY created_at ASC
                "#,
            )
            .map_err(|error| PersistenceError::new("prepare generation results", error))?;
        let rows = statement
            .query_map(params![task_id], |row| {
                Ok(GenerationResultRecord {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    asset_id: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|error| PersistenceError::new("query generation results", error))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(
                row.map_err(|error| PersistenceError::new("read generation result row", error))?,
            );
        }
        Ok(records)
    }

    fn update_progress(
        &mut self,
        task_id: &str,
        progress: u8,
    ) -> Result<GenerationTaskRecord, GenerationRepositoryError> {
        let now = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|error| {
                PersistenceError::new("format generation progress timestamp", error)
            })?;
        let record = self
            .connection
            .query_row(
                "UPDATE generation_tasks SET progress = ?1, updated_at = ?2 \
                 WHERE id = ?3 AND deleted_at IS NULL \
                 RETURNING id, workspace_id, provider_name, model_name, prompt_text, \
                 status, error_message, progress, revision, created_at, updated_at, \
                 started_at, completed_at",
                params![progress as i64, now, task_id],
                map_task_record,
            )
            .map_err(|error| {
                if let rusqlite::Error::QueryReturnedNoRows = error {
                    GenerationRepositoryError::TaskNotFound(task_id.to_owned())
                } else {
                    PersistenceError::new("update generation progress", error).into()
                }
            })?;
        Ok(record)
    }
}

fn is_unique_violation(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(err, _) if err.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

fn map_task_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<GenerationTaskRecord> {
    let status_str: String = row.get(5)?;
    let status = GenerationStatus::parse(&status_str).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(GenerationTaskRecord {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        provider_name: row.get(2)?,
        model_name: row.get(3)?,
        prompt_text: row.get(4)?,
        status,
        error_message: row.get(6)?,
        progress: row.get::<_, i64>(7)? as u8,
        revision: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        started_at: row.get(11)?,
        completed_at: row.get(12)?,
    })
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::adapters::sqlite::asset_repository::SqliteAssetRepository;
    use crate::adapters::sqlite::workspace_repository::SqliteWorkspaceRepository;
    use crate::domain::generation::GenerationFilter;
    use crate::domain::workspace::NewWorkspace;
    use crate::ports::asset_repository::{AssetRepository, ImportOptions};
    use crate::ports::workspace_repository::WorkspaceRepository;
    use std::fs;

    const SAMPLE_WORKSPACE_ID: &str = "00000000-0000-4000-8000-000000000001";

    fn seed() -> (tempfile::TempDir, SqliteGenerationRepository, String) {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let mut workspace = SqliteWorkspaceRepository::open(&path).unwrap();
        workspace
            .initialize(&NewWorkspace::try_new("测试工作空间", "测试教师").unwrap())
            .unwrap();
        let workspace_id = workspace
            .get_status()
            .unwrap()
            .workspace
            .unwrap()
            .workspace_id
            .clone();
        drop(workspace);
        let repository = SqliteGenerationRepository::open(&path).unwrap();
        (directory, repository, workspace_id)
    }

    fn seed_with_asset() -> (
        tempfile::TempDir,
        SqliteGenerationRepository,
        String,
        String,
    ) {
        let (directory, repository, workspace_id) = seed();
        let source_file = directory.path().join("output.png");
        fs::write(&source_file, b"fake-png-content").unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let mut asset_repository = SqliteAssetRepository::open(&path).unwrap();
        let summary = asset_repository
            .import(
                vec![source_file.to_string_lossy().into_owned()],
                "generation",
                ImportOptions::default(),
            )
            .unwrap();
        assert_eq!(summary.imported.len(), 1);
        let asset_id = summary.imported[0].asset.id.clone();
        drop(asset_repository);
        (directory, repository, workspace_id, asset_id)
    }

    #[test]
    fn creates_and_reads_back_a_pending_task() {
        let (_dir, mut repo, workspace_id) = seed();
        let draft = GenerationTaskDraft::try_new(
            workspace_id.clone(),
            "Seedance".to_owned(),
            "seedance-v2".to_owned(),
            "生成一段舞蹈视频".to_owned(),
        )
        .unwrap();
        let task = repo.create_task(draft).unwrap();
        assert_eq!(task.status, GenerationStatus::Pending);
        assert_eq!(task.provider_name, "Seedance");

        let fetched = repo.get_task(&task.id).unwrap().unwrap();
        assert_eq!(fetched.id, task.id);
        assert_eq!(fetched.status, GenerationStatus::Pending);
    }

    #[test]
    fn updates_status_to_running_and_succeeded() {
        let (_dir, mut repo, workspace_id) = seed();
        let draft = GenerationTaskDraft::try_new(
            workspace_id,
            "可灵".to_owned(),
            "kl-v1".to_owned(),
            "prompt".to_owned(),
        )
        .unwrap();
        let task = repo.create_task(draft).unwrap();

        let running = repo
            .update_status(&task.id, GenerationStatus::Running, None)
            .unwrap();
        assert_eq!(running.status, GenerationStatus::Running);
        assert!(running.started_at.is_some());

        let succeeded = repo
            .update_status(&task.id, GenerationStatus::Succeeded, None)
            .unwrap();
        assert_eq!(succeeded.status, GenerationStatus::Succeeded);
        assert!(succeeded.completed_at.is_some());
        assert!(succeeded.error_message.is_none());
    }

    #[test]
    fn marks_failed_with_error_message() {
        let (_dir, mut repo, workspace_id) = seed();
        let draft = GenerationTaskDraft::try_new(
            workspace_id,
            "海螺".to_owned(),
            "hailuo-v1".to_owned(),
            "prompt".to_owned(),
        )
        .unwrap();
        let task = repo.create_task(draft).unwrap();
        let failed = repo
            .update_status(
                &task.id,
                GenerationStatus::Failed,
                Some("供应商返回 500".to_owned()),
            )
            .unwrap();
        assert_eq!(failed.status, GenerationStatus::Failed);
        assert_eq!(failed.error_message.as_deref(), Some("供应商返回 500"));
    }

    #[test]
    fn rejects_update_for_nonexistent_task() {
        let (_dir, mut repo, _workspace_id) = seed();
        let error = repo
            .update_status(SAMPLE_WORKSPACE_ID, GenerationStatus::Failed, None)
            .unwrap_err();
        assert!(matches!(error, GenerationRepositoryError::TaskNotFound(_)));
    }

    #[test]
    fn lists_tasks_filtered_by_status() {
        let (_dir, mut repo, workspace_id) = seed();
        for provider in &["Seedance", "可灵", "海螺"] {
            let draft = GenerationTaskDraft::try_new(
                workspace_id.clone(),
                provider.to_string(),
                "model".to_owned(),
                "prompt".to_owned(),
            )
            .unwrap();
            let task = repo.create_task(draft).unwrap();
            if provider == &"可灵" {
                repo.update_status(&task.id, GenerationStatus::Failed, Some("err".to_owned()))
                    .unwrap();
            }
        }

        let pending = repo
            .list_tasks(&GenerationFilter::try_new(Some("pending".to_owned()), None, None).unwrap())
            .unwrap();
        assert_eq!(pending.len(), 2);

        let failed = repo
            .list_tasks(&GenerationFilter::try_new(Some("failed".to_owned()), None, None).unwrap())
            .unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].provider_name, "可灵");
    }

    #[test]
    fn adds_result_and_lists_for_task() {
        let (_dir, mut repo, workspace_id, asset_id) = seed_with_asset();
        let draft = GenerationTaskDraft::try_new(
            workspace_id,
            "Seedance".to_owned(),
            "model".to_owned(),
            "prompt".to_owned(),
        )
        .unwrap();
        let task = repo.create_task(draft).unwrap();
        let result = repo.add_result(&task.id, &asset_id).unwrap();
        assert_eq!(result.task_id, task.id);
        assert_eq!(result.asset_id, asset_id);

        let results = repo.list_results(&task.id).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, result.id);
    }

    #[test]
    fn rejects_result_for_missing_asset() {
        let (_dir, mut repo, workspace_id) = seed();
        let draft = GenerationTaskDraft::try_new(
            workspace_id,
            "Seedance".to_owned(),
            "model".to_owned(),
            "prompt".to_owned(),
        )
        .unwrap();
        let task = repo.create_task(draft).unwrap();
        let error = repo.add_result(&task.id, SAMPLE_WORKSPACE_ID).unwrap_err();
        assert!(matches!(error, GenerationRepositoryError::AssetNotFound(_)));
    }
}
