#![allow(dead_code)]
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::domain::creative_state::{
    CharacterAsset, CreativeStateDraft, CreativeStateRecord, DecisionRecord, ReferenceAsset,
    SceneAsset, StyleTokens,
};
use crate::ports::{
    creative_state_repository::{CreativeStateRepository, CreativeStateRepositoryError},
    persistence::PersistenceError,
};

pub struct SqliteCreativeStateRepository {
    connection: rusqlite::Connection,
}

impl SqliteCreativeStateRepository {
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

impl CreativeStateRepository for SqliteCreativeStateRepository {
    fn create(
        &mut self,
        draft: CreativeStateDraft,
    ) -> Result<CreativeStateRecord, CreativeStateRepositoryError> {
        let id = Uuid::new_v4().to_string();
        let ts = now()?;
        let style_json = serde_json::to_string(&draft.style_tokens).unwrap_or_default();
        let constraints_json = serde_json::to_string(&draft.constraints).unwrap_or_default();

        self.connection.execute(
            "INSERT INTO creative_states (id, workspace_id, project_name, project_type, audience, platform, style_tokens_json, constraints_json, version, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,1,?9,?9)",
            params![id, draft.workspace_id, draft.project_name, draft.project_type, draft.audience, draft.platform, style_json, constraints_json, ts],
        ).map_err(|e| PersistenceError::new("insert creative state", e))?;

        Ok(CreativeStateRecord {
            id,
            workspace_id: draft.workspace_id,
            project_name: draft.project_name,
            project_type: draft.project_type,
            audience: draft.audience,
            platform: draft.platform,
            style_tokens: draft.style_tokens,
            characters: Vec::new(),
            scenes: Vec::new(),
            references: Vec::new(),
            constraints: draft.constraints,
            history_decisions: Vec::new(),
            version: 1,
            created_at: ts.clone(),
            updated_at: ts,
        })
    }

    fn get(
        &mut self,
        id: &str,
    ) -> Result<Option<CreativeStateRecord>, CreativeStateRepositoryError> {
        self.connection.query_row(
            "SELECT id, workspace_id, project_name, project_type, audience, platform, style_tokens_json, characters_json, scenes_json, references_json, constraints_json, history_decisions_json, version, created_at, updated_at FROM creative_states WHERE id = ?1",
            params![id], map_creative_state,
        ).optional().map_err(|e| PersistenceError::new("get creative state", e).into())
    }

    fn get_by_workspace(
        &mut self,
        workspace_id: &str,
    ) -> Result<Option<CreativeStateRecord>, CreativeStateRepositoryError> {
        self.connection.query_row(
            "SELECT id, workspace_id, project_name, project_type, audience, platform, style_tokens_json, characters_json, scenes_json, references_json, constraints_json, history_decisions_json, version, created_at, updated_at FROM creative_states WHERE workspace_id = ?1 ORDER BY updated_at DESC LIMIT 1",
            params![workspace_id], map_creative_state,
        ).optional().map_err(|e| PersistenceError::new("get creative state by workspace", e).into())
    }

    fn update(&mut self, record: &CreativeStateRecord) -> Result<(), CreativeStateRepositoryError> {
        let ts = now()?;
        let style_json = serde_json::to_string(&record.style_tokens).unwrap_or_default();
        let characters_json = serde_json::to_string(&record.characters).unwrap_or_default();
        let scenes_json = serde_json::to_string(&record.scenes).unwrap_or_default();
        let references_json = serde_json::to_string(&record.references).unwrap_or_default();
        let constraints_json = serde_json::to_string(&record.constraints).unwrap_or_default();
        let decisions_json = serde_json::to_string(&record.history_decisions).unwrap_or_default();

        let rows = self.connection.execute(
            "UPDATE creative_states SET project_name=?1, project_type=?2, audience=?3, platform=?4, style_tokens_json=?5, characters_json=?6, scenes_json=?7, references_json=?8, constraints_json=?9, history_decisions_json=?10, version=version+1, updated_at=?11 WHERE id=?12 AND version=?13",
            params![
                record.project_name, record.project_type, record.audience, record.platform,
                style_json, characters_json, scenes_json, references_json,
                constraints_json, decisions_json, ts, record.id, record.version
            ],
        ).map_err(|e| PersistenceError::new("update creative state", e))?;

        if rows == 0 {
            return Err(CreativeStateRepositoryError::NotFound(record.id.clone()));
        }
        Ok(())
    }

    fn delete(&mut self, id: &str) -> Result<(), CreativeStateRepositoryError> {
        self.connection
            .execute("DELETE FROM creative_states WHERE id = ?1", params![id])
            .map_err(|e| PersistenceError::new("delete creative state", e))?;
        Ok(())
    }
}

fn map_creative_state(row: &rusqlite::Row<'_>) -> rusqlite::Result<CreativeStateRecord> {
    let style_json: String = row.get(6)?;
    let characters_json: String = row.get(7)?;
    let scenes_json: String = row.get(8)?;
    let references_json: String = row.get(9)?;
    let constraints_json: String = row.get(10)?;
    let decisions_json: String = row.get(11)?;

    Ok(CreativeStateRecord {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        project_name: row.get(2)?,
        project_type: row.get(3)?,
        audience: row.get(4)?,
        platform: row.get(5)?,
        style_tokens: serde_json::from_str(&style_json).unwrap_or_default(),
        characters: serde_json::from_str(&characters_json).unwrap_or_default(),
        scenes: serde_json::from_str(&scenes_json).unwrap_or_default(),
        references: serde_json::from_str(&references_json).unwrap_or_default(),
        constraints: serde_json::from_str(&constraints_json).unwrap_or_default(),
        history_decisions: serde_json::from_str(&decisions_json).unwrap_or_default(),
        version: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}
