//! SqliteSemanticRepository — SemanticRepository 的 SQLite 实现。
//!
//! 持久化 ArtifactSemanticProfile 和 RelationCandidate。

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use crate::application::error::AppError;
use crate::domain::common::InferenceSource;
use crate::domain::relation_candidate::{
    CandidateStatus, RelationCandidate, RelationCandidateDraft, RelationSource,
};
use crate::domain::semantic::{ArtifactSemanticProfile, SemanticEntity, SemanticTag};
use crate::ports::persistence::PersistenceError;
use crate::ports::reloadable::Reloadable;
use crate::ports::semantic_repository::SemanticRepository;

// ──────────────────────────────────────────────────────────────────
// SqliteSemanticRepository
// ──────────────────────────────────────────────────────────────────

pub struct SqliteSemanticRepository {
    connection: Mutex<Connection>,
}

impl SqliteSemanticRepository {
    pub fn open(connection: Connection) -> Self {
        Self {
            connection: Mutex::new(connection),
        }
    }
}

impl Reloadable for SqliteSemanticRepository {
    fn reload(&self, _database_path: &Path) -> Result<(), AppError> {
        Ok(())
    }
}

// ── Mapping helpers ──

fn map_profile(row: &rusqlite::Row) -> rusqlite::Result<ArtifactSemanticProfile> {
    let tags_json: String = row.get(3)?;
    let entities_json: String = row.get(4)?;
    let objects_json: String = row.get(11)?;
    let scene_json: String = row.get(12)?;
    let actions_json: String = row.get(13)?;
    let concepts_json: String = row.get(14)?;
    let relations_json: String = row.get(15)?;

    let tags: Vec<SemanticTag> = serde_json::from_str(&tags_json).unwrap_or_default();
    let entities: Vec<SemanticEntity> = serde_json::from_str(&entities_json).unwrap_or_default();
    let objects: Vec<String> = serde_json::from_str(&objects_json).unwrap_or_default();
    let scene: Vec<String> = serde_json::from_str(&scene_json).unwrap_or_default();
    let actions: Vec<String> = serde_json::from_str(&actions_json).unwrap_or_default();
    let concepts: Vec<String> = serde_json::from_str(&concepts_json).unwrap_or_default();
    let relations: Vec<crate::domain::semantic::SemanticRelation> =
        serde_json::from_str(&relations_json).unwrap_or_default();

    Ok(ArtifactSemanticProfile {
        artifact_id: row.get(0)?,
        caption: row.get(1)?,
        ocr_text: row.get(2)?,
        tags,
        entities,
        embedding_id: row.get(5)?,
        analyzer: row.get(6)?,
        analyzer_version: row.get(7)?,
        analyzed_at: row.get(8)?,
        // 新增字段：从数据库读取
        description_short: row.get(9)?,
        description_detailed: row.get(10)?,
        objects,
        scene,
        actions,
        concepts,
        relations,
        analysis_job_id: row.get(16)?,
    })
}

fn map_candidate(row: &rusqlite::Row) -> rusqlite::Result<RelationCandidate> {
    let source_str: String = row.get(6)?;
    let status_str: String = row.get(7)?;

    let source = RelationSource::parse(&source_str).unwrap_or(RelationSource::System);
    let status = CandidateStatus::parse(&status_str).unwrap_or(CandidateStatus::Pending);

    Ok(RelationCandidate {
        id: row.get(0)?,
        source_node_id: row.get(1)?,
        target_node_id: row.get(2)?,
        relation_type: row.get(3)?,
        confidence: row.get(4)?,
        evidence_json: row.get(5)?,
        source,
        status,
        created_at: row.get(8)?,
        reviewed_at: row.get(9)?,
    })
}

impl SemanticRepository for SqliteSemanticRepository {
    // ── ArtifactSemanticProfile ──

    fn upsert_profile(&self, profile: &ArtifactSemanticProfile) -> Result<(), AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let tags_json = serde_json::to_string(&profile.tags).unwrap_or_else(|_| "[]".to_owned());
        let entities_json =
            serde_json::to_string(&profile.entities).unwrap_or_else(|_| "[]".to_owned());
        let objects_json =
            serde_json::to_string(&profile.objects).unwrap_or_else(|_| "[]".to_owned());
        let scene_json = serde_json::to_string(&profile.scene).unwrap_or_else(|_| "[]".to_owned());
        let actions_json =
            serde_json::to_string(&profile.actions).unwrap_or_else(|_| "[]".to_owned());
        let concepts_json =
            serde_json::to_string(&profile.concepts).unwrap_or_else(|_| "[]".to_owned());
        let relations_json =
            serde_json::to_string(&profile.relations).unwrap_or_else(|_| "[]".to_owned());

        connection
            .execute(
                "INSERT INTO artifact_semantic_profiles (artifact_id, caption, ocr_text, tags_json, entities_json, embedding_id, analyzer, analyzer_version, analyzed_at, description_short, description_detailed, objects_json, scene_json, actions_json, concepts_json, relations_json, analysis_job_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17) ON CONFLICT(artifact_id) DO UPDATE SET caption = excluded.caption, ocr_text = excluded.ocr_text, tags_json = excluded.tags_json, entities_json = excluded.entities_json, embedding_id = excluded.embedding_id, analyzer = excluded.analyzer, analyzer_version = excluded.analyzer_version, analyzed_at = excluded.analyzed_at, description_short = excluded.description_short, description_detailed = excluded.description_detailed, objects_json = excluded.objects_json, scene_json = excluded.scene_json, actions_json = excluded.actions_json, concepts_json = excluded.concepts_json, relations_json = excluded.relations_json, analysis_job_id = excluded.analysis_job_id",
                params![
                    profile.artifact_id,
                    profile.caption,
                    profile.ocr_text,
                    tags_json,
                    entities_json,
                    profile.embedding_id,
                    profile.analyzer,
                    profile.analyzer_version,
                    profile.analyzed_at,
                    profile.description_short,
                    profile.description_detailed,
                    objects_json,
                    scene_json,
                    actions_json,
                    concepts_json,
                    relations_json,
                    profile.analysis_job_id,
                ],
            )
            .map_err(|e| PersistenceError::new("upsert_profile", e))?;

        Ok(())
    }

    fn get_profile(&self, artifact_id: &str) -> Result<Option<ArtifactSemanticProfile>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let result = connection
            .query_row(
                "SELECT artifact_id, caption, ocr_text, tags_json, entities_json, embedding_id, analyzer, analyzer_version, analyzed_at, description_short, description_detailed, objects_json, scene_json, actions_json, concepts_json, relations_json, analysis_job_id FROM artifact_semantic_profiles WHERE artifact_id = ?1",
                params![artifact_id],
                map_profile,
            )
            .optional()
            .map_err(|e| PersistenceError::new("get_profile", e))?;

        Ok(result)
    }

    fn delete_profile(&self, artifact_id: &str) -> Result<(), AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        connection
            .execute(
                "DELETE FROM artifact_semantic_profiles WHERE artifact_id = ?1",
                params![artifact_id],
            )
            .map_err(|e| PersistenceError::new("delete_profile", e))?;
        Ok(())
    }

    fn find_profiles_by_tag(
        &self,
        tag_query: &str,
        limit: usize,
    ) -> Result<Vec<ArtifactSemanticProfile>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        // JSON search: match tag names containing the query string
        let pattern = format!("%{tag_query}%");
        let mut stmt = connection
            .prepare(
                "SELECT artifact_id, caption, ocr_text, tags_json, entities_json, embedding_id, analyzer, analyzer_version, analyzed_at, description_short, description_detailed, objects_json, scene_json, actions_json, concepts_json, relations_json, analysis_job_id FROM artifact_semantic_profiles WHERE tags_json LIKE ?1 LIMIT ?2",
            )
            .map_err(|e| PersistenceError::new("find_profiles_by_tag", e))?;

        let rows = stmt
            .query_map(params![pattern, limit as i64], map_profile)
            .map_err(|e| PersistenceError::new("find_profiles_by_tag", e))?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    fn find_profiles_by_ocr(
        &self,
        text_query: &str,
        limit: usize,
    ) -> Result<Vec<ArtifactSemanticProfile>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let pattern = format!("%{text_query}%");
        let mut stmt = connection
            .prepare(
                "SELECT artifact_id, caption, ocr_text, tags_json, entities_json, embedding_id, analyzer, analyzer_version, analyzed_at, description_short, description_detailed, objects_json, scene_json, actions_json, concepts_json, relations_json, analysis_job_id FROM artifact_semantic_profiles WHERE ocr_text LIKE ?1 OR caption LIKE ?1 LIMIT ?2",
            )
            .map_err(|e| PersistenceError::new("find_profiles_by_ocr", e))?;

        let rows = stmt
            .query_map(params![pattern, limit as i64], map_profile)
            .map_err(|e| PersistenceError::new("find_profiles_by_ocr", e))?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    fn find_profile_by_embedding(
        &self,
        embedding_id: &str,
    ) -> Result<Option<ArtifactSemanticProfile>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let result = connection
            .query_row(
                "SELECT artifact_id, caption, ocr_text, tags_json, entities_json, embedding_id, analyzer, analyzer_version, analyzed_at, description_short, description_detailed, objects_json, scene_json, actions_json, concepts_json, relations_json, analysis_job_id FROM artifact_semantic_profiles WHERE embedding_id = ?1",
                params![embedding_id],
                map_profile,
            )
            .optional()
            .map_err(|e| PersistenceError::new("find_profile_by_embedding", e))?;

        Ok(result)
    }

    // ── RelationCandidate ──

    fn create_candidate(
        &self,
        draft: &RelationCandidateDraft,
    ) -> Result<RelationCandidate, AppError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = crate::adapters::sqlite::now_rfc3339().map_err(|e| AppError::StateUnavailable)?;
        let source_str = draft.source.as_str();

        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        connection
            .execute(
                "INSERT INTO canvas_relation_candidates (id, source_node_id, target_node_id, relation_type, confidence, evidence_json, source, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8)",
                params![
                    id,
                    draft.source_node_id,
                    draft.target_node_id,
                    draft.relation_type,
                    draft.confidence,
                    draft.evidence_json,
                    source_str,
                    now,
                ],
            )
            .map_err(|e| PersistenceError::new("create_candidate", e))?;

        Ok(RelationCandidate {
            id,
            source_node_id: draft.source_node_id.clone(),
            target_node_id: draft.target_node_id.clone(),
            relation_type: draft.relation_type.clone(),
            confidence: draft.confidence,
            evidence_json: draft.evidence_json.clone(),
            source: draft.source,
            status: CandidateStatus::Pending,
            created_at: now,
            reviewed_at: None,
        })
    }

    fn get_candidate(&self, id: &str) -> Result<Option<RelationCandidate>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let result = connection
            .query_row(
                "SELECT id, source_node_id, target_node_id, relation_type, confidence, evidence_json, source, status, created_at, reviewed_at FROM canvas_relation_candidates WHERE id = ?1",
                params![id],
                map_candidate,
            )
            .optional()
            .map_err(|e| PersistenceError::new("get_candidate", e))?;

        Ok(result)
    }

    fn list_pending_candidates(&self, limit: usize) -> Result<Vec<RelationCandidate>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let mut stmt = connection
            .prepare(
                "SELECT id, source_node_id, target_node_id, relation_type, confidence, evidence_json, source, status, created_at, reviewed_at FROM canvas_relation_candidates WHERE status = 'pending' ORDER BY confidence DESC LIMIT ?1",
            )
            .map_err(|e| PersistenceError::new("list_pending_candidates", e))?;

        let rows = stmt
            .query_map(params![limit as i64], map_candidate)
            .map_err(|e| PersistenceError::new("list_pending_candidates", e))?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    fn list_candidates_for_node(&self, node_id: &str) -> Result<Vec<RelationCandidate>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let mut stmt = connection
            .prepare(
                "SELECT id, source_node_id, target_node_id, relation_type, confidence, evidence_json, source, status, created_at, reviewed_at FROM canvas_relation_candidates WHERE (source_node_id = ?1 OR target_node_id = ?1) ORDER BY confidence DESC",
            )
            .map_err(|e| PersistenceError::new("list_candidates_for_node", e))?;

        let rows = stmt
            .query_map(params![node_id], map_candidate)
            .map_err(|e| PersistenceError::new("list_candidates_for_node", e))?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    fn update_candidate_status(&self, id: &str, status: CandidateStatus) -> Result<(), AppError> {
        let now = crate::adapters::sqlite::now_rfc3339().map_err(|e| AppError::StateUnavailable)?;
        let status_str = status.as_str();

        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        connection
            .execute(
                "UPDATE canvas_relation_candidates SET status = ?1, reviewed_at = ?2 WHERE id = ?3",
                params![status_str, now, id],
            )
            .map_err(|e| PersistenceError::new("update_candidate_status", e))?;

        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE artifact_semantic_profiles (
                artifact_id TEXT PRIMARY KEY,
                caption TEXT,
                ocr_text TEXT,
                tags_json TEXT NOT NULL DEFAULT '[]',
                entities_json TEXT NOT NULL DEFAULT '[]',
                embedding_id TEXT,
                analyzer TEXT NOT NULL,
                analyzer_version TEXT NOT NULL,
                analyzed_at TEXT NOT NULL,
                description_short TEXT,
                description_detailed TEXT,
                objects_json TEXT NOT NULL DEFAULT '[]',
                scene_json TEXT NOT NULL DEFAULT '[]',
                actions_json TEXT NOT NULL DEFAULT '[]',
                concepts_json TEXT NOT NULL DEFAULT '[]',
                relations_json TEXT NOT NULL DEFAULT '[]',
                analysis_job_id TEXT
            );
            CREATE TABLE canvas_relation_candidates (
                id TEXT PRIMARY KEY,
                source_node_id TEXT NOT NULL,
                target_node_id TEXT NOT NULL,
                relation_type TEXT NOT NULL,
                confidence REAL NOT NULL,
                evidence_json TEXT,
                source TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL,
                reviewed_at TEXT
            );
            ",
        )
        .unwrap();
        conn
    }

    #[test]
    fn upsert_and_get_profile() {
        let repo = SqliteSemanticRepository::open(setup_db());
        let profile = ArtifactSemanticProfile {
            artifact_id: "art-1".to_owned(),
            caption: Some("A girl in rain".to_owned()),
            ocr_text: None,
            tags: vec![SemanticTag {
                name: "rain".to_owned(),
                confidence: 0.9,
                source: InferenceSource::Llm,
            }],
            entities: vec![],
            embedding_id: Some("emb-1".to_owned()),
            analyzer: "gpt-4o".to_owned(),
            analyzer_version: "v1".to_owned(),
            analyzed_at: "2026-08-17T00:00:00Z".to_owned(),
            description_short: None,
            description_detailed: None,
            objects: vec![],
            scene: vec![],
            actions: vec![],
            concepts: vec![],
            relations: vec![],
            analysis_job_id: None,
        };
        repo.upsert_profile(&profile).unwrap();
        let got = repo.get_profile("art-1").unwrap().unwrap();
        assert_eq!(got.artifact_id, "art-1");
        assert_eq!(got.caption, Some("A girl in rain".to_owned()));
        assert_eq!(got.tags.len(), 1);
        assert_eq!(got.tags[0].name, "rain");
    }

    #[test]
    fn find_by_tag() {
        let repo = SqliteSemanticRepository::open(setup_db());
        let profile = ArtifactSemanticProfile {
            artifact_id: "art-1".to_owned(),
            caption: None,
            ocr_text: None,
            tags: vec![SemanticTag {
                name: "character:Alice".to_owned(),
                confidence: 0.95,
                source: InferenceSource::Llm,
            }],
            entities: vec![],
            embedding_id: None,
            analyzer: "gpt-4o".to_owned(),
            analyzer_version: "v1".to_owned(),
            analyzed_at: "2026-08-17T00:00:00Z".to_owned(),
            description_short: None,
            description_detailed: None,
            objects: vec![],
            scene: vec![],
            actions: vec![],
            concepts: vec![],
            relations: vec![],
            analysis_job_id: None,
        };
        repo.upsert_profile(&profile).unwrap();

        let found = repo.find_profiles_by_tag("Alice", 10).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].artifact_id, "art-1");

        let not_found = repo.find_profiles_by_tag("Bob", 10).unwrap();
        assert!(not_found.is_empty());
    }

    #[test]
    fn create_and_list_candidates() {
        let repo = SqliteSemanticRepository::open(setup_db());
        let draft = RelationCandidateDraft {
            source_node_id: "n1".to_owned(),
            target_node_id: "n2".to_owned(),
            relation_type: "same_character".to_owned(),
            confidence: 0.94,
            evidence_json: Some(r#"["white hair"]"#.to_owned()),
            source: RelationSource::Rag,
        };
        let candidate = repo.create_candidate(&draft).unwrap();
        assert_eq!(candidate.status, CandidateStatus::Pending);

        let pending = repo.list_pending_candidates(10).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, candidate.id);
    }

    #[test]
    fn update_candidate_status() {
        let repo = SqliteSemanticRepository::open(setup_db());
        let draft = RelationCandidateDraft {
            source_node_id: "n1".to_owned(),
            target_node_id: "n2".to_owned(),
            relation_type: "same_style".to_owned(),
            confidence: 0.8,
            evidence_json: None,
            source: RelationSource::Rag,
        };
        let candidate = repo.create_candidate(&draft).unwrap();

        repo.update_candidate_status(&candidate.id, CandidateStatus::Accepted)
            .unwrap();

        let updated = repo.get_candidate(&candidate.id).unwrap().unwrap();
        assert_eq!(updated.status, CandidateStatus::Accepted);
        assert!(updated.reviewed_at.is_some());

        // Should no longer be in pending
        let pending = repo.list_pending_candidates(10).unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn list_candidates_for_node() {
        let repo = SqliteSemanticRepository::open(setup_db());
        let draft1 = RelationCandidateDraft {
            source_node_id: "target".to_owned(),
            target_node_id: "other".to_owned(),
            relation_type: "same_character".to_owned(),
            confidence: 0.9,
            evidence_json: None,
            source: RelationSource::Rag,
        };
        let draft2 = RelationCandidateDraft {
            source_node_id: "other".to_owned(),
            target_node_id: "target".to_owned(),
            relation_type: "same_style".to_owned(),
            confidence: 0.7,
            evidence_json: None,
            source: RelationSource::Rag,
        };
        repo.create_candidate(&draft1).unwrap();
        repo.create_candidate(&draft2).unwrap();

        let found = repo.list_candidates_for_node("target").unwrap();
        assert_eq!(found.len(), 2);
    }
}
