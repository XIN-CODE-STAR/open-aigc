//! CanvasDomainService — 画布领域服务（IPC 层用）。
//!
//! 包装 CanvasRepository，提供面向 IPC 的 CRUD 操作。
//! 与 CanvasContextService（Agent 工具用）不同，此服务直接暴露完整的 CRUD。

use std::sync::{Arc, Mutex};

use crate::{
    application::error::AppError,
    domain::canvas::{
        CanvasDraft, CanvasEdge, CanvasEdgeDraft, CanvasNode, CanvasNodeDraft, CanvasRecord,
        NodePatch,
    },
    ports::canvas_repository::CanvasRepository,
};

// ──────────────────────────────────────────────────────────────────
// CanvasDomainService
// ──────────────────────────────────────────────────────────────────

pub struct CanvasDomainService {
    repo: Arc<Mutex<dyn CanvasRepository>>,
}

impl CanvasDomainService {
    pub fn new(repo: Arc<Mutex<dyn CanvasRepository>>) -> Self {
        Self { repo }
    }

    // ── Canvas CRUD ──

    pub fn create_canvas(
        &self,
        workspace_id: &str,
        name: &str,
        description: Option<&str>,
        conversation_ref: Option<&str>,
    ) -> Result<CanvasRecord, AppError> {
        let draft = CanvasDraft {
            workspace_id: workspace_id.to_owned(),
            name: name.to_owned(),
            description: description.map(|s| s.to_owned()),
            conversation_ref: conversation_ref.map(|s| s.to_owned()),
        };
        let repo = self.repo.lock().map_err(|_| AppError::StateUnavailable)?;
        repo.create_canvas(draft)
    }

    pub fn list_canvases(&self, workspace_id: &str) -> Result<Vec<CanvasRecord>, AppError> {
        let repo = self.repo.lock().map_err(|_| AppError::StateUnavailable)?;
        repo.list_canvases(workspace_id)
    }

    pub fn get_canvas(&self, id: &str) -> Result<Option<CanvasRecord>, AppError> {
        let repo = self.repo.lock().map_err(|_| AppError::StateUnavailable)?;
        repo.get_canvas(id)
    }

    pub fn delete_canvas(&self, id: &str) -> Result<(), AppError> {
        let repo = self.repo.lock().map_err(|_| AppError::StateUnavailable)?;
        repo.delete_canvas(id)
    }

    // ── Node CRUD ──

    pub fn add_node(&self, draft: CanvasNodeDraft) -> Result<CanvasNode, AppError> {
        let repo = self.repo.lock().map_err(|_| AppError::StateUnavailable)?;
        repo.add_node(draft)
    }

    pub fn update_node(&self, node_id: &str, patch: NodePatch) -> Result<CanvasNode, AppError> {
        let repo = self.repo.lock().map_err(|_| AppError::StateUnavailable)?;
        repo.update_node(node_id, patch)
    }

    pub fn delete_node(&self, node_id: &str) -> Result<(), AppError> {
        let repo = self.repo.lock().map_err(|_| AppError::StateUnavailable)?;
        repo.delete_node(node_id)
    }

    pub fn list_nodes(&self, canvas_id: &str) -> Result<Vec<CanvasNode>, AppError> {
        let repo = self.repo.lock().map_err(|_| AppError::StateUnavailable)?;
        repo.list_nodes(canvas_id)
    }

    // ── Edge CRUD ──

    pub fn add_edge(&self, draft: CanvasEdgeDraft) -> Result<CanvasEdge, AppError> {
        let repo = self.repo.lock().map_err(|_| AppError::StateUnavailable)?;
        repo.add_edge(draft)
    }

    pub fn delete_edge(&self, edge_id: &str) -> Result<(), AppError> {
        let repo = self.repo.lock().map_err(|_| AppError::StateUnavailable)?;
        repo.delete_edge(edge_id)
    }

    pub fn list_edges(&self, canvas_id: &str) -> Result<Vec<CanvasEdge>, AppError> {
        let repo = self.repo.lock().map_err(|_| AppError::StateUnavailable)?;
        repo.list_edges(canvas_id)
    }
}
