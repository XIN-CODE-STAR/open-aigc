use std::path::Path;
use std::sync::Mutex;

use crate::{
    adapters::sqlite::memory_canvas_repository::SqliteMemoryCanvasRepository,
    application::error::AppError,
    ports::{
        memory_canvas_repository::{
            CanvasDraft, EdgeDraft, MemoryCanvas, MemoryCanvasRepository, MemoryEdge, MemoryNode,
            MemoryViewport, NodeDraft,
        },
        reloadable::Reloadable,
    },
};

pub struct MemoryCanvasService {
    repository: Mutex<SqliteMemoryCanvasRepository>,
}

impl MemoryCanvasService {
    pub fn new(repository: SqliteMemoryCanvasRepository) -> Self {
        Self {
            repository: Mutex::new(repository),
        }
    }

    pub fn create_canvas(
        &self,
        workspace_id: &str,
        name: &str,
        description: Option<&str>,
    ) -> Result<MemoryCanvas, AppError> {
        let draft = CanvasDraft {
            workspace_id: workspace_id.to_owned(),
            name: name.to_owned(),
            description: description.map(|s| s.to_owned()),
        };
        self.repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?
            .create_canvas(draft)
    }

    pub fn list_canvases(&self, workspace_id: &str) -> Result<Vec<MemoryCanvas>, AppError> {
        self.repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?
            .list_canvases(workspace_id)
    }

    pub fn get_canvas(&self, id: &str) -> Result<Option<MemoryCanvas>, AppError> {
        self.repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?
            .get_canvas(id)
    }

    pub fn delete_canvas(&self, id: &str) -> Result<(), AppError> {
        self.repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?
            .delete_canvas(id)
    }

    pub fn add_node(
        &self,
        canvas_id: &str,
        node_type: &str,
        position_x: f64,
        position_y: f64,
        payload_json: &str,
        summary: Option<&str>,
        asset_id: Option<&str>,
    ) -> Result<MemoryNode, AppError> {
        let draft = NodeDraft {
            canvas_id: canvas_id.to_owned(),
            node_type: node_type.to_owned(),
            position_x,
            position_y,
            width: None,
            height: None,
            payload_json: payload_json.to_owned(),
            summary: summary.map(|s| s.to_owned()),
            asset_id: asset_id.map(|s| s.to_owned()),
        };
        self.repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?
            .add_node(draft)
    }

    pub fn update_node(
        &self,
        id: &str,
        canvas_id: &str,
        node_type: &str,
        position_x: f64,
        position_y: f64,
        payload_json: &str,
        summary: Option<&str>,
        asset_id: Option<&str>,
    ) -> Result<MemoryNode, AppError> {
        let draft = NodeDraft {
            canvas_id: canvas_id.to_owned(),
            node_type: node_type.to_owned(),
            position_x,
            position_y,
            width: None,
            height: None,
            payload_json: payload_json.to_owned(),
            summary: summary.map(|s| s.to_owned()),
            asset_id: asset_id.map(|s| s.to_owned()),
        };
        self.repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?
            .update_node(id, draft)
    }

    pub fn delete_node(&self, id: &str) -> Result<(), AppError> {
        self.repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?
            .delete_node(id)
    }

    pub fn list_nodes(&self, canvas_id: &str) -> Result<Vec<MemoryNode>, AppError> {
        self.repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?
            .list_nodes(canvas_id)
    }

    pub fn add_edge(
        &self,
        canvas_id: &str,
        source_node_id: &str,
        target_node_id: &str,
        edge_type: &str,
        label: Option<&str>,
    ) -> Result<MemoryEdge, AppError> {
        let draft = EdgeDraft {
            canvas_id: canvas_id.to_owned(),
            source_node_id: source_node_id.to_owned(),
            target_node_id: target_node_id.to_owned(),
            edge_type: edge_type.to_owned(),
            label: label.map(|s| s.to_owned()),
        };
        self.repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?
            .add_edge(draft)
    }

    pub fn delete_edge(&self, id: &str) -> Result<(), AppError> {
        self.repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?
            .delete_edge(id)
    }

    pub fn list_edges(&self, canvas_id: &str) -> Result<Vec<MemoryEdge>, AppError> {
        self.repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?
            .list_edges(canvas_id)
    }

    pub fn save_viewport(
        &self,
        canvas_id: &str,
        zoom: f64,
        pan_x: f64,
        pan_y: f64,
    ) -> Result<(), AppError> {
        self.repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?
            .save_viewport(canvas_id, zoom, pan_x, pan_y)
    }

    pub fn get_viewport(&self, canvas_id: &str) -> Result<MemoryViewport, AppError> {
        self.repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?
            .get_viewport(canvas_id)
    }
}

impl Reloadable for MemoryCanvasService {
    fn reload(&self, _database_path: &Path) -> Result<(), AppError> {
        Ok(())
    }
}
