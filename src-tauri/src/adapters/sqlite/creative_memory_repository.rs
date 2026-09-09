use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use super::now_rfc3339;
use crate::{
    domain::creative_memory::{
        CreativeMemoryDraft, CreativeMemoryEventRecord, CreativeMemoryRecord, MemoryEventType,
        MemoryFilter, MemoryScope, MemorySource, MemoryStatus, MemoryType,
    },
    ports::{
        creative_memory_repository::{CreativeMemoryRepository, CreativeMemoryRepositoryError},
        persistence::PersistenceError,
    },
};

pub struct SqliteCreativeMemoryRepository {
    connection: Connection,
}

impl SqliteCreativeMemoryRepository {
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        use crate::adapters::sqlite::database::open_database;
        let connection = open_database(database_path.as_ref())?;
        Ok(Self { connection })
    }
}

impl CreativeMemoryRepository for SqliteCreativeMemoryRepository {
    fn insert(
        &mut self,
        draft: &CreativeMemoryDraft,
    ) -> Result<CreativeMemoryRecord, CreativeMemoryRepositoryError> {
        let id = Uuid::new_v4().to_string();
        let now = now_rfc3339()?;

        self.connection
            .execute(
                r#"
                INSERT INTO creative_memory (
                  id, memory_type, scope, scope_ref_id, content_json, summary,
                  source, source_ref_id, status, confidence, confirm_count,
                  created_by, created_at, updated_at
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
                "#,
                params![
                    id,
                    draft.memory_type.as_str(),
                    draft.scope.as_str(),
                    draft.scope_ref_id,
                    draft.content_json,
                    draft.summary,
                    draft.source.as_str(),
                    draft.source_ref_id,
                    MemoryStatus::Active.as_str(),
                    0.5_f64,
                    1_i64,
                    draft.created_by,
                    now,
                    now,
                ],
            )
            .map_err(|e| PersistenceError::new("insert creative memory", e))?;

        // 插入创建事件
        self.connection
            .execute(
                r#"
                INSERT INTO creative_memory_events (
                  id, memory_id, event_type, event_detail, created_by, created_at
                ) VALUES (?1,?2,?3,?4,?5,?6)
                "#,
                params![
                    Uuid::new_v4().to_string(),
                    id,
                    MemoryEventType::Created.as_str(),
                    "用户保存创意记忆",
                    draft.created_by,
                    now,
                ],
            )
            .map_err(|e| PersistenceError::new("insert memory event", e))?;

        self.connection
            .query_row(
                "SELECT * FROM creative_memory WHERE id = ?1",
                params![id],
                map_memory_record,
            )
            .map_err(|e| PersistenceError::new("read created memory", e).into())
    }

    fn get(
        &mut self,
        id: &str,
    ) -> Result<Option<CreativeMemoryRecord>, CreativeMemoryRepositoryError> {
        self.connection
            .query_row(
                "SELECT * FROM creative_memory WHERE id = ?1",
                params![id],
                map_memory_record,
            )
            .optional()
            .map_err(|e| PersistenceError::new("read creative memory", e).into())
    }

    fn list(
        &mut self,
        filter: &MemoryFilter,
    ) -> Result<Vec<CreativeMemoryRecord>, CreativeMemoryRepositoryError> {
        let mut sql = String::from("SELECT * FROM creative_memory WHERE status != 'archived'");
        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_idx = 1;

        if let Some(scope) = filter.scope {
            sql.push_str(&format!(" AND scope = ?{param_idx}"));
            params_vec.push(Box::new(scope.as_str().to_owned()));
            param_idx += 1;
        }

        if let Some(ref scope_ref_id) = filter.scope_ref_id {
            sql.push_str(&format!(" AND scope_ref_id = ?{param_idx}"));
            params_vec.push(Box::new(scope_ref_id.clone()));
            param_idx += 1;
        }

        if let Some(memory_type) = filter.memory_type {
            sql.push_str(&format!(" AND memory_type = ?{param_idx}"));
            params_vec.push(Box::new(memory_type.as_str().to_owned()));
            param_idx += 1;
        }

        if let Some(status) = filter.status {
            sql.push_str(&format!(" AND status = ?{param_idx}"));
            params_vec.push(Box::new(status.as_str().to_owned()));
            param_idx += 1;
        }

        sql.push_str(&format!(" ORDER BY updated_at DESC LIMIT ?{param_idx}"));
        params_vec.push(Box::new(filter.limit));

        let mut stmt = self
            .connection
            .prepare(&sql)
            .map_err(|e| PersistenceError::new("prepare memory list", e))?;

        let rows = stmt
            .query_map(
                rusqlite::params_from_iter(params_vec.iter().map(|b| b.as_ref())),
                map_memory_record,
            )
            .map_err(|e| PersistenceError::new("query memory list", e))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| PersistenceError::new("read memory row", e))?);
        }
        Ok(records)
    }

    fn update_status(
        &mut self,
        id: &str,
        status: MemoryStatus,
    ) -> Result<CreativeMemoryRecord, CreativeMemoryRepositoryError> {
        let now = now_rfc3339()?;
        let affected = self
            .connection
            .execute(
                "UPDATE creative_memory SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status.as_str(), now, id],
            )
            .map_err(|e| PersistenceError::new("update memory status", e))?;

        if affected == 0 {
            return Err(CreativeMemoryRepositoryError::NotFound(id.to_owned()));
        }

        self.connection
            .query_row(
                "SELECT * FROM creative_memory WHERE id = ?1",
                params![id],
                map_memory_record,
            )
            .map_err(|e| PersistenceError::new("read updated memory", e).into())
    }

    fn update_content(
        &mut self,
        id: &str,
        content_json: &str,
        summary: &str,
    ) -> Result<CreativeMemoryRecord, CreativeMemoryRepositoryError> {
        let now = now_rfc3339()?;
        let affected = self
            .connection
            .execute(
                "UPDATE creative_memory SET content_json = ?1, summary = ?2, updated_at = ?3 WHERE id = ?4",
                params![content_json, summary, now, id],
            )
            .map_err(|e| PersistenceError::new("update memory content", e))?;

        if affected == 0 {
            return Err(CreativeMemoryRepositoryError::NotFound(id.to_owned()));
        }

        self.connection
            .query_row(
                "SELECT * FROM creative_memory WHERE id = ?1",
                params![id],
                map_memory_record,
            )
            .map_err(|e| PersistenceError::new("read updated memory", e).into())
    }

    fn confirm(&mut self, id: &str) -> Result<CreativeMemoryRecord, CreativeMemoryRepositoryError> {
        let now = now_rfc3339()?;
        let affected = self
            .connection
            .execute(
                r#"
                UPDATE creative_memory
                SET confirm_count = confirm_count + 1,
                    confidence = MIN(1.0, confidence + 0.1),
                    updated_at = ?1
                WHERE id = ?2
                "#,
                params![now, id],
            )
            .map_err(|e| PersistenceError::new("confirm memory", e))?;

        if affected == 0 {
            return Err(CreativeMemoryRepositoryError::NotFound(id.to_owned()));
        }

        self.connection
            .query_row(
                "SELECT * FROM creative_memory WHERE id = ?1",
                params![id],
                map_memory_record,
            )
            .map_err(|e| PersistenceError::new("read confirmed memory", e).into())
    }

    fn archive(&mut self, id: &str) -> Result<(), CreativeMemoryRepositoryError> {
        let now = now_rfc3339()?;
        let affected = self
            .connection
            .execute(
                "UPDATE creative_memory SET status = 'archived', updated_at = ?1 WHERE id = ?2",
                params![now, id],
            )
            .map_err(|e| PersistenceError::new("archive memory", e))?;

        if affected == 0 {
            return Err(CreativeMemoryRepositoryError::NotFound(id.to_owned()));
        }
        Ok(())
    }

    fn insert_event(
        &mut self,
        memory_id: &str,
        event_type: MemoryEventType,
        event_detail: Option<&str>,
        created_by: &str,
    ) -> Result<CreativeMemoryEventRecord, CreativeMemoryRepositoryError> {
        let id = Uuid::new_v4().to_string();
        let now = now_rfc3339()?;

        self.connection
            .execute(
                r#"
                INSERT INTO creative_memory_events (
                  id, memory_id, event_type, event_detail, created_by, created_at
                ) VALUES (?1,?2,?3,?4,?5,?6)
                "#,
                params![
                    id,
                    memory_id,
                    event_type.as_str(),
                    event_detail,
                    created_by,
                    now
                ],
            )
            .map_err(|e| PersistenceError::new("insert memory event", e))?;

        self.connection
            .query_row(
                "SELECT * FROM creative_memory_events WHERE id = ?1",
                params![id],
                map_event_record,
            )
            .map_err(|e| PersistenceError::new("read created event", e).into())
    }

    fn list_events(
        &mut self,
        memory_id: &str,
    ) -> Result<Vec<CreativeMemoryEventRecord>, CreativeMemoryRepositoryError> {
        let mut stmt = self
            .connection
            .prepare(
                "SELECT * FROM creative_memory_events WHERE memory_id = ?1 ORDER BY created_at DESC",
            )
            .map_err(|e| PersistenceError::new("prepare events list", e))?;

        let rows = stmt
            .query_map(params![memory_id], map_event_record)
            .map_err(|e| PersistenceError::new("query events list", e))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| PersistenceError::new("read event row", e))?);
        }
        Ok(records)
    }
}

// ─────────────────────────────────────────────────────
// 行映射
// ─────────────────────────────────────────────────────

fn map_memory_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<CreativeMemoryRecord> {
    let type_str: String = row.get(1)?;
    let scope_str: String = row.get(2)?;
    let source_str: String = row.get(6)?;
    let status_str: String = row.get(8)?;

    Ok(CreativeMemoryRecord {
        id: row.get(0)?,
        memory_type: MemoryType::parse(&type_str).unwrap_or(MemoryType::StylePreference),
        scope: MemoryScope::parse(&scope_str).unwrap_or(MemoryScope::User),
        scope_ref_id: row.get(3)?,
        content_json: row.get(4)?,
        summary: row.get(5)?,
        source: MemorySource::parse(&source_str).unwrap_or(MemorySource::ExplicitSave),
        source_ref_id: row.get(7)?,
        status: MemoryStatus::parse(&status_str).unwrap_or(MemoryStatus::Active),
        confidence: row.get(9)?,
        confirm_count: row.get(10)?,
        created_by: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn map_event_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<CreativeMemoryEventRecord> {
    Ok(CreativeMemoryEventRecord {
        id: row.get(0)?,
        memory_id: row.get(1)?,
        event_type: MemoryEventType::Created, // 简化处理
        event_detail: row.get(3)?,
        created_by: row.get(4)?,
        created_at: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::{
        adapters::sqlite::workspace_repository::SqliteWorkspaceRepository,
        domain::workspace::NewWorkspace, ports::workspace_repository::WorkspaceRepository,
    };

    const SAMPLE_UUID: &str = "123e4567-e89b-42d3-a456-426614174000";

    fn seed() -> (tempfile::TempDir, SqliteCreativeMemoryRepository) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("workspace.sqlite3");
        let mut ws = SqliteWorkspaceRepository::open(&path).unwrap();
        ws.initialize(&NewWorkspace::try_new("测试", "老师").unwrap())
            .unwrap();
        drop(ws);
        let repo = SqliteCreativeMemoryRepository::open(&path).unwrap();
        (dir, repo)
    }

    #[test]
    fn inserts_and_reads_memory() {
        let (_dir, mut repo) = seed();
        let draft = CreativeMemoryDraft::try_new(
            "style_preference".into(),
            "user".into(),
            None,
            r#"{"color_palette": ["warm", "muted"]}"#.into(),
            "偏好暖色调".into(),
            "explicit_save".into(),
            None,
            SAMPLE_UUID.into(),
        )
        .unwrap();

        let record = repo.insert(&draft).unwrap();
        assert_eq!(record.memory_type, MemoryType::StylePreference);
        assert_eq!(record.scope, MemoryScope::User);
        assert_eq!(record.status, MemoryStatus::Active);
        assert_eq!(record.confidence, 0.5);

        let fetched = repo.get(&record.id).unwrap().unwrap();
        assert_eq!(fetched.summary, "偏好暖色调");
    }

    #[test]
    fn lists_active_memories() {
        let (_dir, mut repo) = seed();

        for i in 0..3 {
            let draft = CreativeMemoryDraft::try_new(
                "style_preference".into(),
                "user".into(),
                None,
                format!(r#"{{"item": {i}}}"#),
                format!("记忆 {i}"),
                "explicit_save".into(),
                None,
                SAMPLE_UUID.into(),
            )
            .unwrap();
            repo.insert(&draft).unwrap();
        }

        let filter = MemoryFilter::try_new(None, None, None, None, None).unwrap();
        let all = repo.list(&filter).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn confirms_memory() {
        let (_dir, mut repo) = seed();
        let draft = CreativeMemoryDraft::try_new(
            "style_preference".into(),
            "user".into(),
            None,
            r#"{"key": "value"}"#.into(),
            "test".into(),
            "explicit_save".into(),
            None,
            SAMPLE_UUID.into(),
        )
        .unwrap();
        let record = repo.insert(&draft).unwrap();
        assert_eq!(record.confirm_count, 1);

        let confirmed = repo.confirm(&record.id).unwrap();
        assert_eq!(confirmed.confirm_count, 2);
        assert!(confirmed.confidence > 0.5);
    }

    #[test]
    fn archives_memory() {
        let (_dir, mut repo) = seed();
        let draft = CreativeMemoryDraft::try_new(
            "style_preference".into(),
            "user".into(),
            None,
            r#"{"key": "value"}"#.into(),
            "test".into(),
            "explicit_save".into(),
            None,
            SAMPLE_UUID.into(),
        )
        .unwrap();
        let record = repo.insert(&draft).unwrap();

        repo.archive(&record.id).unwrap();

        // archived 记录不会出现在默认列表中
        let filter = MemoryFilter::try_new(None, None, None, None, None).unwrap();
        let active = repo.list(&filter).unwrap();
        assert!(active.is_empty());
    }
}
