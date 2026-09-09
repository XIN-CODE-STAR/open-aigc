use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use crate::{
    adapters::sqlite::database::open_database,
    application::error::AppError,
    domain::canvas::{
        CanvasDraft, CanvasEdge, CanvasEdgeDraft, CanvasEdgeKind, CanvasNode, CanvasNodeDraft,
        CanvasNodeKind, CanvasNodeRefs, CanvasNodeStatus, CanvasPosition, CanvasRecord, NodePatch,
    },
    ports::{
        canvas_repository::CanvasRepository, persistence::PersistenceError, reloadable::Reloadable,
    },
};

// ──────────────────────────────────────────────────────────────────
// SqliteCanvasRepository
// ──────────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub struct SqliteCanvasRepository {
    connection: Mutex<Connection>,
}

impl SqliteCanvasRepository {
    pub fn open(database_path: &Path) -> Result<Self, AppError> {
        let connection = open_database(database_path)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}

impl Reloadable for SqliteCanvasRepository {
    fn reload(&self, _database_path: &Path) -> Result<(), AppError> {
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

fn parse_kind(s: &str) -> CanvasNodeKind {
    CanvasNodeKind::parse(s).unwrap_or(CanvasNodeKind::Note)
}

fn parse_edge_kind(s: &str) -> CanvasEdgeKind {
    CanvasEdgeKind::parse(s).unwrap_or(CanvasEdgeKind::Reference)
}

fn parse_status(s: Option<&str>) -> Option<CanvasNodeStatus> {
    s.and_then(|v| CanvasNodeStatus::parse(v).ok())
}

/// Map a row to a CanvasNode (SELECT column order must match).
fn map_node(row: &rusqlite::Row) -> rusqlite::Result<CanvasNode> {
    let kind_str: String = row.get(2)?;
    let status_str: Option<String> = row.get(8)?;
    let metadata_json: String = row.get(18)?;

    let metadata: HashMap<String, serde_json::Value> =
        serde_json::from_str(&metadata_json).unwrap_or_default();

    Ok(CanvasNode {
        id: row.get(0)?,
        canvas_id: row.get(1)?,
        kind: parse_kind(&kind_str),
        position: CanvasPosition {
            x: row.get(3)?,
            y: row.get(4)?,
        },
        size: match (row.get::<_, Option<f64>>(5)?, row.get::<_, Option<f64>>(6)?) {
            (Some(w), Some(h)) => Some(crate::domain::canvas::CanvasSize {
                width: w,
                height: h,
            }),
            _ => None,
        },
        summary: row.get(7)?,
        description: row.get(9)?,
        prompt: row.get(10)?,
        status: parse_status(status_str.as_deref()),
        refs: CanvasNodeRefs {
            memory_id: row.get(11)?,
            artifact_id: row.get(12)?,
            task_id: row.get(13)?,
            shot_id: row.get(14)?,
            scene_id: row.get(15)?,
            project_id: row.get(16)?,
            agent_id: row.get(17)?,
            asset_id: row.get(19)?,
            conversation_id: row.get(20)?,
        },
        metadata,
        revision: 0, // Will be read from DB after V31 migration
        created_at: row.get(21)?,
        updated_at: row.get(22)?,
    })
}

/// SQL SELECT clause for canvas_nodes (must match map_node column order).
const NODE_SELECT: &str = "SELECT id, canvas_id, kind, position_x, position_y, width, height, summary, status, description, prompt, memory_ref, artifact_ref, task_ref, shot_ref, scene_ref, project_ref, agent_ref, metadata_json, asset_ref, conversation_ref, created_at, updated_at FROM canvas_nodes";

/// Map a row to a CanvasEdge.
fn map_edge(row: &rusqlite::Row) -> rusqlite::Result<CanvasEdge> {
    let kind_str: String = row.get(4)?;
    let metadata_json: String = row.get(6)?;

    let metadata: HashMap<String, serde_json::Value> =
        serde_json::from_str(&metadata_json).unwrap_or_default();

    Ok(CanvasEdge {
        id: row.get(0)?,
        canvas_id: row.get(1)?,
        source_node_id: row.get(2)?,
        target_node_id: row.get(3)?,
        kind: parse_edge_kind(&kind_str),
        label: row.get(5)?,
        metadata,
        created_at: row.get(7)?,
    })
}

const EDGE_SELECT: &str = "SELECT id, canvas_id, source_node_id, target_node_id, kind, label, metadata_json, created_at FROM canvas_edges";

// ──────────────────────────────────────────────────────────────────
// CanvasRepository impl
// ──────────────────────────────────────────────────────────────────

impl CanvasRepository for SqliteCanvasRepository {
    // ── Canvas CRUD ──

    fn create_canvas(&self, draft: CanvasDraft) -> Result<CanvasRecord, AppError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_rfc3339();

        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        connection
            .execute(
                "INSERT INTO canvas_canvases (id, workspace_id, name, description, conversation_ref, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                params![id, draft.workspace_id, draft.name, draft.description, draft.conversation_ref, now],
            )
            .map_err(|e| PersistenceError::new("create_canvas", e))?;

        Ok(CanvasRecord {
            id,
            workspace_id: draft.workspace_id,
            name: draft.name,
            description: draft.description,
            conversation_ref: draft.conversation_ref,
            node_count: 0,
            edge_count: 0,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    fn list_canvases(&self, workspace_id: &str) -> Result<Vec<CanvasRecord>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let mut stmt = connection
            .prepare(
                "SELECT id, workspace_id, name, description, node_count, edge_count, created_at, updated_at FROM canvas_canvases WHERE workspace_id = ?1 AND deleted_at IS NULL ORDER BY updated_at DESC",
            )
            .map_err(|e| PersistenceError::new("list_canvases", e))?;

        let rows = stmt
            .query_map(params![workspace_id], |row| {
                Ok(CanvasRecord {
                    id: row.get(0)?,
                    workspace_id: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    conversation_ref: None, // Will be read from DB after V32 migration
                    node_count: row.get(4)?,
                    edge_count: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })
            .map_err(|e| PersistenceError::new("list_canvases", e))?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    fn get_canvas(&self, id: &str) -> Result<Option<CanvasRecord>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let mut stmt = connection
            .prepare(
                "SELECT id, workspace_id, name, description, node_count, edge_count, created_at, updated_at FROM canvas_canvases WHERE id = ?1 AND deleted_at IS NULL",
            )
            .map_err(|e| PersistenceError::new("get_canvas", e))?;

        let result = stmt
            .query_row(params![id], |row| {
                Ok(CanvasRecord {
                    id: row.get(0)?,
                    workspace_id: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    conversation_ref: None, // Will be read from DB after V32 migration
                    node_count: row.get(4)?,
                    edge_count: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })
            .optional()
            .map_err(|e| PersistenceError::new("get_canvas", e))?;

        Ok(result)
    }

    fn delete_canvas(&self, id: &str) -> Result<(), AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let now = now_rfc3339();

        connection
            .execute(
                "UPDATE canvas_canvases SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
                params![now, id],
            )
            .map_err(|e| PersistenceError::new("delete_canvas", e))?;

        Ok(())
    }

    // ── Node CRUD ──

    fn add_node(&self, draft: CanvasNodeDraft) -> Result<CanvasNode, AppError> {
        draft
            .validate()
            .map_err(|e| AppError::AgentToolFailed(e.to_string()))?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_rfc3339();
        let metadata_json =
            serde_json::to_string(&draft.metadata).unwrap_or_else(|_| "{}".to_owned());

        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        connection
            .execute(
                "INSERT INTO canvas_nodes (
                    id, canvas_id, kind, position_x, position_y, width, height,
                    summary, description, prompt, status,
                    memory_ref, artifact_ref, task_ref, shot_ref, scene_ref, project_ref, agent_ref, asset_ref, conversation_ref,
                    metadata_json, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?22)",
                params![
                    id, draft.canvas_id, draft.kind.as_str(),
                    draft.position.x, draft.position.y,
                    draft.size.map(|s| s.width), draft.size.map(|s| s.height),
                    draft.summary, draft.description, draft.prompt,
                    draft.status.map(|s| s.as_str().to_owned()),
                    draft.refs.memory_id, draft.refs.artifact_id, draft.refs.task_id,
                    draft.refs.shot_id, draft.refs.scene_id, draft.refs.project_id,
                    draft.refs.agent_id, draft.refs.asset_id, draft.refs.conversation_id,
                    metadata_json, now,
                ],
            )
            .map_err(|e| PersistenceError::new("add_node", e))?;

        // Update canvas node count
        connection
            .execute(
                "UPDATE canvas_canvases SET node_count = node_count + 1, updated_at = ?1 WHERE id = ?2",
                params![now, draft.canvas_id],
            )
            .map_err(|e| PersistenceError::new("add_node_counter", e))?;

        Ok(CanvasNode {
            id,
            canvas_id: draft.canvas_id,
            kind: draft.kind,
            position: draft.position,
            size: draft.size,
            summary: draft.summary,
            description: draft.description,
            prompt: draft.prompt,
            status: draft.status,
            refs: draft.refs,
            metadata: draft.metadata,
            revision: 0,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    fn update_node(&self, id: &str, patch: NodePatch) -> Result<CanvasNode, AppError> {
        let now = now_rfc3339();
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;

        // Read current node, apply patch, write back
        let current = self.get_node_internal(&connection, id)?;

        let node = current.ok_or_else(|| {
            PersistenceError::new("update_node", rusqlite::Error::QueryReturnedNoRows)
        })?;

        // Apply patch to existing node
        let kind = patch.kind.unwrap_or(node.kind);
        let pos = patch.position.unwrap_or(node.position);
        let size = patch.size.or(node.size);
        let summary = patch.summary.or(node.summary);
        let description = patch.description.or(node.description);
        let prompt = patch.prompt.or(node.prompt);
        let status = patch.status.or(node.status);
        let refs = patch.refs.unwrap_or(node.refs);
        let metadata = patch.metadata.unwrap_or(node.metadata);
        let metadata_json = serde_json::to_string(&metadata).unwrap_or_else(|_| "{}".to_owned());

        connection
            .execute(
                "UPDATE canvas_nodes SET
                    kind = ?2, position_x = ?3, position_y = ?4, width = ?5, height = ?6,
                    summary = ?7, description = ?8, prompt = ?9, status = ?10,
                    memory_ref = ?11, artifact_ref = ?12, task_ref = ?13,
                    shot_ref = ?14, scene_ref = ?15, project_ref = ?16,
                    agent_ref = ?17, asset_ref = ?18, conversation_ref = ?19,
                    metadata_json = ?20, updated_at = ?1
                WHERE id = ?21 AND deleted_at IS NULL",
                params![
                    now,
                    kind.as_str(),
                    pos.x,
                    pos.y,
                    size.map(|s| s.width),
                    size.map(|s| s.height),
                    summary,
                    description,
                    prompt,
                    status.map(|s| s.as_str().to_owned()),
                    refs.memory_id,
                    refs.artifact_id,
                    refs.task_id,
                    refs.shot_id,
                    refs.scene_id,
                    refs.project_id,
                    refs.agent_id,
                    refs.asset_id,
                    refs.conversation_id,
                    metadata_json,
                    id,
                ],
            )
            .map_err(|e| PersistenceError::new("update_node", e))?;

        Ok(CanvasNode {
            id: id.to_owned(),
            canvas_id: node.canvas_id,
            kind,
            position: pos,
            size,
            summary,
            description,
            prompt,
            status,
            refs,
            metadata,
            revision: node.revision + 1,
            created_at: node.created_at,
            updated_at: now,
        })
    }

    fn delete_node(&self, id: &str) -> Result<(), AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let now = now_rfc3339();

        // Get canvas_id before deleting for counter update
        let canvas_id: Option<String> = connection
            .query_row(
                "SELECT canvas_id FROM canvas_nodes WHERE id = ?1 AND deleted_at IS NULL",
                params![id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| PersistenceError::new("delete_node_lookup", e))?;

        connection
            .execute(
                "UPDATE canvas_nodes SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
                params![now, id],
            )
            .map_err(|e| PersistenceError::new("delete_node", e))?;

        // Also soft-delete edges connected to this node
        connection
            .execute(
                "UPDATE canvas_edges SET deleted_at = ?1 WHERE (source_node_id = ?2 OR target_node_id = ?2) AND deleted_at IS NULL",
                params![now, id],
            )
            .map_err(|e| PersistenceError::new("delete_node_edges", e))?;

        // Update canvas counts
        if let Some(cid) = canvas_id {
            connection
                .execute(
                    "UPDATE canvas_canvases SET node_count = MAX(0, node_count - 1), updated_at = ?1 WHERE id = ?2",
                    params![now, cid],
                )
                .map_err(|e| PersistenceError::new("delete_node_counter", e))?;
        }

        Ok(())
    }

    fn list_nodes(&self, canvas_id: &str) -> Result<Vec<CanvasNode>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let mut stmt = connection
            .prepare(&format!(
                "{NODE_SELECT} WHERE canvas_id = ?1 AND deleted_at IS NULL ORDER BY created_at"
            ))
            .map_err(|e| PersistenceError::new("list_nodes", e))?;

        let rows = stmt
            .query_map(params![canvas_id], map_node)
            .map_err(|e| PersistenceError::new("list_nodes", e))?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    fn get_node(&self, id: &str) -> Result<Option<CanvasNode>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        self.get_node_internal(&connection, id)
    }

    // ── Edge CRUD ──

    fn add_edge(&self, draft: CanvasEdgeDraft) -> Result<CanvasEdge, AppError> {
        draft
            .validate()
            .map_err(|e| AppError::AgentToolFailed(e.to_string()))?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_rfc3339();
        let metadata_json =
            serde_json::to_string(&draft.metadata).unwrap_or_else(|_| "{}".to_owned());

        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        connection
            .execute(
                "INSERT INTO canvas_edges (id, canvas_id, source_node_id, target_node_id, kind, label, metadata_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![id, draft.canvas_id, draft.source_node_id, draft.target_node_id, draft.kind.as_str(), draft.label, metadata_json, now],
            )
            .map_err(|e| PersistenceError::new("add_edge", e))?;

        // Update canvas edge count
        connection
            .execute(
                "UPDATE canvas_canvases SET edge_count = edge_count + 1, updated_at = ?1 WHERE id = ?2",
                params![now, draft.canvas_id],
            )
            .map_err(|e| PersistenceError::new("add_edge_counter", e))?;

        Ok(CanvasEdge {
            id,
            canvas_id: draft.canvas_id,
            source_node_id: draft.source_node_id,
            target_node_id: draft.target_node_id,
            kind: draft.kind,
            label: draft.label,
            metadata: draft.metadata,
            created_at: now,
        })
    }

    fn delete_edge(&self, id: &str) -> Result<(), AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let now = now_rfc3339();

        connection
            .execute(
                "UPDATE canvas_edges SET deleted_at = ?1 WHERE id = ?2",
                params![now, id],
            )
            .map_err(|e| PersistenceError::new("delete_edge", e))?;

        Ok(())
    }

    fn list_edges(&self, canvas_id: &str) -> Result<Vec<CanvasEdge>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let mut stmt = connection
            .prepare(&format!(
                "{EDGE_SELECT} WHERE canvas_id = ?1 AND deleted_at IS NULL ORDER BY created_at"
            ))
            .map_err(|e| PersistenceError::new("list_edges", e))?;

        let rows = stmt
            .query_map(params![canvas_id], map_edge)
            .map_err(|e| PersistenceError::new("list_edges", e))?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    // ── Batch operations ──

    fn add_nodes_batch(&self, drafts: Vec<CanvasNodeDraft>) -> Result<Vec<CanvasNode>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let now = now_rfc3339();
        let mut results = Vec::with_capacity(drafts.len());

        for draft in &drafts {
            draft
                .validate()
                .map_err(|e| AppError::AgentToolFailed(e.to_string()))?;
        }

        for draft in drafts {
            let id = uuid::Uuid::new_v4().to_string();
            let metadata_json =
                serde_json::to_string(&draft.metadata).unwrap_or_else(|_| "{}".to_owned());

            connection
                .execute(
                    "INSERT INTO canvas_nodes (
                        id, canvas_id, kind, position_x, position_y, width, height,
                        summary, description, prompt, status,
                        memory_ref, artifact_ref, task_ref, shot_ref, scene_ref, project_ref, agent_ref, asset_ref, conversation_ref,
                        metadata_json, created_at, updated_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?22)",
                    params![
                        id, draft.canvas_id, draft.kind.as_str(),
                        draft.position.x, draft.position.y,
                        draft.size.map(|s| s.width), draft.size.map(|s| s.height),
                        draft.summary, draft.description, draft.prompt,
                        draft.status.map(|s| s.as_str().to_owned()),
                        draft.refs.memory_id, draft.refs.artifact_id, draft.refs.task_id,
                        draft.refs.shot_id, draft.refs.scene_id, draft.refs.project_id,
                        draft.refs.agent_id, draft.refs.asset_id, draft.refs.conversation_id,
                        metadata_json, now,
                    ],
                )
                .map_err(|e| PersistenceError::new("add_nodes_batch", e))?;

            results.push(CanvasNode {
                id,
                canvas_id: draft.canvas_id,
                kind: draft.kind,
                position: draft.position,
                size: draft.size,
                summary: draft.summary,
                description: draft.description,
                prompt: draft.prompt,
                status: draft.status,
                refs: draft.refs,
                metadata: draft.metadata,
                revision: 0,
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        }

        // Update canvas node count in bulk
        if let Some(cid) = results.first().map(|n| &n.canvas_id) {
            connection
                .execute(
                    "UPDATE canvas_canvases SET node_count = (SELECT COUNT(*) FROM canvas_nodes WHERE canvas_id = ?1 AND deleted_at IS NULL), updated_at = ?2 WHERE id = ?1",
                    params![cid, now],
                )
                .map_err(|e| PersistenceError::new("add_nodes_batch_counter", e))?;
        }

        Ok(results)
    }

    fn add_edges_batch(&self, drafts: Vec<CanvasEdgeDraft>) -> Result<Vec<CanvasEdge>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let now = now_rfc3339();
        let mut results = Vec::with_capacity(drafts.len());

        for draft in &drafts {
            draft
                .validate()
                .map_err(|e| AppError::AgentToolFailed(e.to_string()))?;
        }

        for draft in drafts {
            let id = uuid::Uuid::new_v4().to_string();
            let metadata_json =
                serde_json::to_string(&draft.metadata).unwrap_or_else(|_| "{}".to_owned());

            connection
                .execute(
                    "INSERT INTO canvas_edges (id, canvas_id, source_node_id, target_node_id, kind, label, metadata_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![id, draft.canvas_id, draft.source_node_id, draft.target_node_id, draft.kind.as_str(), draft.label, metadata_json, now],
                )
                .map_err(|e| PersistenceError::new("add_edges_batch", e))?;

            results.push(CanvasEdge {
                id,
                canvas_id: draft.canvas_id,
                source_node_id: draft.source_node_id,
                target_node_id: draft.target_node_id,
                kind: draft.kind,
                label: draft.label,
                metadata: draft.metadata,
                created_at: now.clone(),
            });
        }

        // Update canvas edge count
        if let Some(cid) = results.first().map(|e| &e.canvas_id) {
            connection
                .execute(
                    "UPDATE canvas_canvases SET edge_count = (SELECT COUNT(*) FROM canvas_edges WHERE canvas_id = ?1 AND deleted_at IS NULL), updated_at = ?2 WHERE id = ?1",
                    params![cid, now],
                )
                .map_err(|e| PersistenceError::new("add_edges_batch_counter", e))?;
        }

        Ok(results)
    }

    // ── Ref queries ──

    fn find_nodes_by_ref(
        &self,
        canvas_id: &str,
        ref_field: &str,
        ref_value: &str,
    ) -> Result<Vec<CanvasNode>, AppError> {
        // Validate ref_field is a known column name to prevent SQL injection
        let valid_refs = [
            "memory_ref",
            "artifact_ref",
            "task_ref",
            "shot_ref",
            "scene_ref",
            "project_ref",
            "agent_ref",
            "asset_ref",
            "conversation_ref",
        ];
        if !valid_refs.contains(&ref_field) {
            return Err(PersistenceError::new(
                "find_nodes_by_ref",
                rusqlite::Error::InvalidParameterName(ref_field.to_owned()),
            )
            .into());
        }

        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let sql = format!(
            "{NODE_SELECT} WHERE canvas_id = ?1 AND {ref_field} = ?2 AND deleted_at IS NULL ORDER BY created_at"
        );
        let mut stmt = connection
            .prepare(&sql)
            .map_err(|e| PersistenceError::new("find_nodes_by_ref", e))?;

        let rows = stmt
            .query_map(params![canvas_id, ref_value], map_node)
            .map_err(|e| PersistenceError::new("find_nodes_by_ref", e))?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    fn find_nodes_by_kind(
        &self,
        canvas_id: &str,
        kind: CanvasNodeKind,
    ) -> Result<Vec<CanvasNode>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let mut stmt = connection
            .prepare(&format!(
                "{NODE_SELECT} WHERE canvas_id = ?1 AND kind = ?2 AND deleted_at IS NULL ORDER BY created_at"
            ))
            .map_err(|e| PersistenceError::new("find_nodes_by_kind", e))?;

        let rows = stmt
            .query_map(params![canvas_id, kind.as_str()], map_node)
            .map_err(|e| PersistenceError::new("find_nodes_by_kind", e))?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    fn find_canvas_by_conversation_ref(
        &self,
        conversation_ref: &str,
    ) -> Result<Option<CanvasRecord>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let result = connection
            .query_row(
                "SELECT id, workspace_id, name, description, conversation_ref, node_count, edge_count, created_at, updated_at FROM canvas_canvases WHERE conversation_ref = ?1 AND deleted_at IS NULL",
                params![conversation_ref],
                |row| {
                    Ok(CanvasRecord {
                        id: row.get(0)?,
                        workspace_id: row.get(1)?,
                        name: row.get(2)?,
                        description: row.get(3)?,
                        conversation_ref: row.get(4)?,
                        node_count: row.get(5)?,
                        edge_count: row.get(6)?,
                        created_at: row.get(7)?,
                        updated_at: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(|e| PersistenceError::new("find_canvas_by_conversation_ref", e))?;

        Ok(result)
    }
}

// ──────────────────────────────────────────────────────────────────
// Internal helpers
// ──────────────────────────────────────────────────────────────────

impl SqliteCanvasRepository {
    fn get_node_internal(
        &self,
        connection: &Connection,
        id: &str,
    ) -> Result<Option<CanvasNode>, AppError> {
        let mut stmt = connection
            .prepare(&format!(
                "{NODE_SELECT} WHERE id = ?1 AND deleted_at IS NULL"
            ))
            .map_err(|e| PersistenceError::new("get_node", e))?;

        let result = stmt
            .query_row(params![id], map_node)
            .optional()
            .map_err(|e| PersistenceError::new("get_node", e))?;

        Ok(result)
    }
}
