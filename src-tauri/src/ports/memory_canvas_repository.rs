use std::path::Path;

use crate::{application::error::AppError, ports::reloadable::Reloadable};

// ──────────────────────────────────────────────────────────────────
// Domain types
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCanvas {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub description: Option<String>,
    pub node_count: i64,
    pub edge_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryNode {
    pub id: String,
    pub canvas_id: String,
    pub node_type: String,
    pub position_x: f64,
    pub position_y: f64,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub payload_json: String,
    pub summary: Option<String>,
    pub asset_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEdge {
    pub id: String,
    pub canvas_id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub edge_type: String,
    pub label: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryViewport {
    pub canvas_id: String,
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
    pub updated_at: String,
}

// ──────────────────────────────────────────────────────────────────
// Draft types
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CanvasDraft {
    pub workspace_id: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NodeDraft {
    pub canvas_id: String,
    pub node_type: String,
    pub position_x: f64,
    pub position_y: f64,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub payload_json: String,
    pub summary: Option<String>,
    pub asset_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EdgeDraft {
    pub canvas_id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub edge_type: String,
    pub label: Option<String>,
}

// ──────────────────────────────────────────────────────────────────
// Repository trait
// ──────────────────────────────────────────────────────────────────

pub trait MemoryCanvasRepository: Send + Reloadable {
    // Canvas CRUD
    fn create_canvas(&self, draft: CanvasDraft) -> Result<MemoryCanvas, AppError>;
    fn list_canvases(&self, workspace_id: &str) -> Result<Vec<MemoryCanvas>, AppError>;
    fn get_canvas(&self, id: &str) -> Result<Option<MemoryCanvas>, AppError>;
    fn delete_canvas(&self, id: &str) -> Result<(), AppError>;

    // Node CRUD
    fn add_node(&self, draft: NodeDraft) -> Result<MemoryNode, AppError>;
    fn update_node(&self, id: &str, draft: NodeDraft) -> Result<MemoryNode, AppError>;
    fn delete_node(&self, id: &str) -> Result<(), AppError>;
    fn list_nodes(&self, canvas_id: &str) -> Result<Vec<MemoryNode>, AppError>;

    // Edge CRUD
    fn add_edge(&self, draft: EdgeDraft) -> Result<MemoryEdge, AppError>;
    fn delete_edge(&self, id: &str) -> Result<(), AppError>;
    fn list_edges(&self, canvas_id: &str) -> Result<Vec<MemoryEdge>, AppError>;

    // Viewport
    fn save_viewport(
        &self,
        canvas_id: &str,
        zoom: f64,
        pan_x: f64,
        pan_y: f64,
    ) -> Result<(), AppError>;
    fn get_viewport(&self, canvas_id: &str) -> Result<MemoryViewport, AppError>;
}
