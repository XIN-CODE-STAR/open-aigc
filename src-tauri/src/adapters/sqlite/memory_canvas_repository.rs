use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use crate::{
    adapters::sqlite::database::open_database,
    application::error::AppError,
    ports::persistence::PersistenceError,
    ports::{
        memory_canvas_repository::{
            CanvasDraft, EdgeDraft, MemoryCanvas, MemoryCanvasRepository, MemoryEdge, MemoryNode,
            MemoryViewport, NodeDraft,
        },
        reloadable::Reloadable,
    },
};

pub struct SqliteMemoryCanvasRepository {
    connection: Mutex<Connection>,
}

impl SqliteMemoryCanvasRepository {
    pub fn open(database_path: &Path) -> Result<Self, AppError> {
        let connection = open_database(database_path)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}

impl Reloadable for SqliteMemoryCanvasRepository {
    fn reload(&self, _database_path: &Path) -> Result<(), AppError> {
        Ok(())
    }
}

impl MemoryCanvasRepository for SqliteMemoryCanvasRepository {
    fn create_canvas(&self, draft: CanvasDraft) -> Result<MemoryCanvas, AppError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();

        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        connection
            .execute(
                "INSERT INTO memory_canvases (id, workspace_id, name, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![id, draft.workspace_id, draft.name, draft.description, now],
            )
            .map_err(|e| PersistenceError::new("operation", e))?;

        Ok(MemoryCanvas {
            id,
            workspace_id: draft.workspace_id,
            name: draft.name,
            description: draft.description,
            node_count: 0,
            edge_count: 0,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    fn list_canvases(&self, workspace_id: &str) -> Result<Vec<MemoryCanvas>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let mut stmt = connection
            .prepare(
                "SELECT id, workspace_id, name, description, node_count, edge_count, created_at, updated_at FROM memory_canvases WHERE workspace_id = ?1 AND deleted_at IS NULL ORDER BY updated_at DESC",
            )
            .map_err(|e| PersistenceError::new("operation", e))?;

        let rows = stmt
            .query_map(params![workspace_id], |row| {
                Ok(MemoryCanvas {
                    id: row.get(0)?,
                    workspace_id: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    node_count: row.get(4)?,
                    edge_count: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })
            .map_err(|e| PersistenceError::new("operation", e))?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    fn get_canvas(&self, id: &str) -> Result<Option<MemoryCanvas>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let mut stmt = connection
            .prepare(
                "SELECT id, workspace_id, name, description, node_count, edge_count, created_at, updated_at FROM memory_canvases WHERE id = ?1 AND deleted_at IS NULL",
            )
            .map_err(|e| PersistenceError::new("operation", e))?;

        let result = stmt
            .query_row(params![id], |row| {
                Ok(MemoryCanvas {
                    id: row.get(0)?,
                    workspace_id: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    node_count: row.get(4)?,
                    edge_count: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })
            .optional()
            .map_err(|e| PersistenceError::new("operation", e))?;

        Ok(result)
    }

    fn delete_canvas(&self, id: &str) -> Result<(), AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();

        connection
            .execute(
                "UPDATE memory_canvases SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
                params![now, id],
            )
            .map_err(|e| PersistenceError::new("operation", e))?;

        Ok(())
    }

    fn add_node(&self, draft: NodeDraft) -> Result<MemoryNode, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();

        connection
            .execute(
                "INSERT INTO memory_nodes (id, canvas_id, node_type, position_x, position_y, width, height, payload_json, summary, asset_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
                params![id, draft.canvas_id, draft.node_type, draft.position_x, draft.position_y, draft.width, draft.height, draft.payload_json, draft.summary, draft.asset_id, now],
            )
            .map_err(|e| PersistenceError::new("operation", e))?;

        // Update canvas node count
        connection
            .execute(
                "UPDATE memory_canvases SET node_count = node_count + 1, updated_at = ?1 WHERE id = ?2",
                params![now, draft.canvas_id],
            )
            .map_err(|e| PersistenceError::new("operation", e))?;

        Ok(MemoryNode {
            id,
            canvas_id: draft.canvas_id,
            node_type: draft.node_type,
            position_x: draft.position_x,
            position_y: draft.position_y,
            width: draft.width,
            height: draft.height,
            payload_json: draft.payload_json,
            summary: draft.summary,
            asset_id: draft.asset_id,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    fn update_node(&self, id: &str, draft: NodeDraft) -> Result<MemoryNode, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();

        connection
            .execute(
                "UPDATE memory_nodes SET node_type = ?1, position_x = ?2, position_y = ?3, width = ?4, height = ?5, payload_json = ?6, summary = ?7, asset_id = ?8, updated_at = ?9 WHERE id = ?10 AND deleted_at IS NULL",
                params![draft.node_type, draft.position_x, draft.position_y, draft.width, draft.height, draft.payload_json, draft.summary, draft.asset_id, now, id],
            )
            .map_err(|e| PersistenceError::new("operation", e))?;

        Ok(MemoryNode {
            id: id.to_owned(),
            canvas_id: draft.canvas_id,
            node_type: draft.node_type,
            position_x: draft.position_x,
            position_y: draft.position_y,
            width: draft.width,
            height: draft.height,
            payload_json: draft.payload_json,
            summary: draft.summary,
            asset_id: draft.asset_id,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    fn delete_node(&self, id: &str) -> Result<(), AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();

        connection
            .execute(
                "UPDATE memory_nodes SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
                params![now, id],
            )
            .map_err(|e| PersistenceError::new("operation", e))?;

        Ok(())
    }

    fn list_nodes(&self, canvas_id: &str) -> Result<Vec<MemoryNode>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let mut stmt = connection
            .prepare(
                "SELECT id, canvas_id, node_type, position_x, position_y, width, height, payload_json, summary, asset_id, created_at, updated_at FROM memory_nodes WHERE canvas_id = ?1 AND deleted_at IS NULL ORDER BY created_at",
            )
            .map_err(|e| PersistenceError::new("operation", e))?;

        let rows = stmt
            .query_map(params![canvas_id], |row| {
                Ok(MemoryNode {
                    id: row.get(0)?,
                    canvas_id: row.get(1)?,
                    node_type: row.get(2)?,
                    position_x: row.get(3)?,
                    position_y: row.get(4)?,
                    width: row.get(5)?,
                    height: row.get(6)?,
                    payload_json: row.get(7)?,
                    summary: row.get(8)?,
                    asset_id: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            })
            .map_err(|e| PersistenceError::new("operation", e))?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    fn add_edge(&self, draft: EdgeDraft) -> Result<MemoryEdge, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();

        connection
            .execute(
                "INSERT INTO memory_edges (id, canvas_id, source_node_id, target_node_id, edge_type, label, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, draft.canvas_id, draft.source_node_id, draft.target_node_id, draft.edge_type, draft.label, now],
            )
            .map_err(|e| PersistenceError::new("operation", e))?;

        // Update canvas edge count
        connection
            .execute(
                "UPDATE memory_canvases SET edge_count = edge_count + 1, updated_at = ?1 WHERE id = ?2",
                params![now, draft.canvas_id],
            )
            .map_err(|e| PersistenceError::new("operation", e))?;

        Ok(MemoryEdge {
            id,
            canvas_id: draft.canvas_id,
            source_node_id: draft.source_node_id,
            target_node_id: draft.target_node_id,
            edge_type: draft.edge_type,
            label: draft.label,
            created_at: now,
        })
    }

    fn delete_edge(&self, id: &str) -> Result<(), AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();

        connection
            .execute(
                "UPDATE memory_edges SET deleted_at = ?1 WHERE id = ?2",
                params![now, id],
            )
            .map_err(|e| PersistenceError::new("operation", e))?;

        Ok(())
    }

    fn list_edges(&self, canvas_id: &str) -> Result<Vec<MemoryEdge>, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let mut stmt = connection
            .prepare(
                "SELECT id, canvas_id, source_node_id, target_node_id, edge_type, label, created_at FROM memory_edges WHERE canvas_id = ?1 AND deleted_at IS NULL ORDER BY created_at",
            )
            .map_err(|e| PersistenceError::new("operation", e))?;

        let rows = stmt
            .query_map(params![canvas_id], |row| {
                Ok(MemoryEdge {
                    id: row.get(0)?,
                    canvas_id: row.get(1)?,
                    source_node_id: row.get(2)?,
                    target_node_id: row.get(3)?,
                    edge_type: row.get(4)?,
                    label: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })
            .map_err(|e| PersistenceError::new("operation", e))?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    fn save_viewport(
        &self,
        canvas_id: &str,
        zoom: f64,
        pan_x: f64,
        pan_y: f64,
    ) -> Result<(), AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();

        connection
            .execute(
                "INSERT OR REPLACE INTO memory_canvas_viewports (canvas_id, zoom, pan_x, pan_y, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![canvas_id, zoom, pan_x, pan_y, now],
            )
            .map_err(|e| PersistenceError::new("operation", e))?;

        Ok(())
    }

    fn get_viewport(&self, canvas_id: &str) -> Result<MemoryViewport, AppError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let mut stmt = connection
            .prepare(
                "SELECT canvas_id, zoom, pan_x, pan_y, updated_at FROM memory_canvas_viewports WHERE canvas_id = ?1",
            )
            .map_err(|e| PersistenceError::new("operation", e))?;

        let result = stmt
            .query_row(params![canvas_id], |row| {
                Ok(MemoryViewport {
                    canvas_id: row.get(0)?,
                    zoom: row.get(1)?,
                    pan_x: row.get(2)?,
                    pan_y: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })
            .optional()
            .map_err(|e| PersistenceError::new("operation", e))?;

        Ok(result.unwrap_or(MemoryViewport {
            canvas_id: canvas_id.to_owned(),
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            updated_at: String::new(),
        }))
    }
}
