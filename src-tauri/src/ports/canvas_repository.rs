//! CanvasRepository 端口 — 画布持久化接口。
//!
//! 新的画布仓库端口，使用 domain/canvas.rs 中的强类型领域模型。
//! 替代 stringly-typed 的 MemoryCanvasRepository。
//!
//! 设计原则：
//! - 使用 &self（内部 Mutex）而非 &mut self（新版模式）
//! - 实现 Reloadable 支持热重载
//! - 支持批量操作（ApplyScriptPlan 需要）
//! - 支持按 refs 查询（CanvasContextService 需要）

use crate::application::error::AppError;
use crate::domain::canvas::{
    CanvasDraft, CanvasEdge, CanvasEdgeDraft, CanvasNode, CanvasNodeDraft, CanvasNodeKind,
    CanvasRecord, NodePatch,
};
use crate::ports::reloadable::Reloadable;

// ──────────────────────────────────────────────────────────────────
// CanvasRepository trait
// ──────────────────────────────────────────────────────────────────

/// 画布持久化仓库端口。
///
/// 所有画布数据的持久化操作都通过此 trait 进行。
/// 实现方（SQLite adapter）负责：
/// - 软删除（deleted_at）
/// - 自动管理 node_count / edge_count 计数器
/// - 约束校验（外键、唯一性）
pub trait CanvasRepository: Send + Sync + Reloadable {
    // ── Canvas CRUD ──

    fn create_canvas(&self, draft: CanvasDraft) -> Result<CanvasRecord, AppError>;
    fn list_canvases(&self, workspace_id: &str) -> Result<Vec<CanvasRecord>, AppError>;
    fn get_canvas(&self, id: &str) -> Result<Option<CanvasRecord>, AppError>;
    fn delete_canvas(&self, id: &str) -> Result<(), AppError>;

    // ── Node CRUD ──

    fn add_node(&self, draft: CanvasNodeDraft) -> Result<CanvasNode, AppError>;
    fn update_node(&self, id: &str, patch: NodePatch) -> Result<CanvasNode, AppError>;
    fn delete_node(&self, id: &str) -> Result<(), AppError>;
    fn list_nodes(&self, canvas_id: &str) -> Result<Vec<CanvasNode>, AppError>;
    fn get_node(&self, id: &str) -> Result<Option<CanvasNode>, AppError>;

    // ── Edge CRUD ──

    fn add_edge(&self, draft: CanvasEdgeDraft) -> Result<CanvasEdge, AppError>;
    fn delete_edge(&self, id: &str) -> Result<(), AppError>;
    fn list_edges(&self, canvas_id: &str) -> Result<Vec<CanvasEdge>, AppError>;

    // ── Batch operations（ApplyScriptPlan 需要）──

    fn add_nodes_batch(&self, drafts: Vec<CanvasNodeDraft>) -> Result<Vec<CanvasNode>, AppError>;
    fn add_edges_batch(&self, drafts: Vec<CanvasEdgeDraft>) -> Result<Vec<CanvasEdge>, AppError>;

    // ── Ref 查询（CanvasContextService 需要）──

    /// 按引用字段查询节点。ref_field 为列名（如 "shot_ref", "project_ref"）。
    fn find_nodes_by_ref(
        &self,
        canvas_id: &str,
        ref_field: &str,
        ref_value: &str,
    ) -> Result<Vec<CanvasNode>, AppError>;

    /// 按节点类型查询。
    fn find_nodes_by_kind(
        &self,
        canvas_id: &str,
        kind: CanvasNodeKind,
    ) -> Result<Vec<CanvasNode>, AppError>;

    // ── Conversation ref 查询 ──

    /// 按 conversation_ref 查找画布（替代 conv-{id} 名称约定）。
    fn find_canvas_by_conversation_ref(
        &self,
        conversation_ref: &str,
    ) -> Result<Option<CanvasRecord>, AppError>;
}
