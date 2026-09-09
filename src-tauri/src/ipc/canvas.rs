//! Canvas IPC — 新画布架构的 Tauri 命令。
//!
//! 替代 memory_canvas.rs，提供 CanvasNode/Edge 领域模型的 CRUD 操作。

use std::collections::HashMap;

use tauri::{AppHandle, Manager};

use crate::{
    application::canvas_domain_service::CanvasDomainService,
    domain::canvas::{
        CanvasEdge, CanvasEdgeDraft, CanvasEdgeKind, CanvasNode, CanvasNodeDraft, CanvasNodeKind,
        CanvasNodeRefs, CanvasNodeStatus, CanvasPosition, CanvasRecord, NodePatch,
    },
};

use super::error::IpcError;

// ──────────────────────────────────────────────────────────────────
// Canvas CRUD
// ──────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct CreateCanvasRequest {
    pub workspace_id: String,
    pub name: String,
    pub description: Option<String>,
    pub conversation_ref: Option<String>,
}

#[tauri::command]
pub async fn canvas_v1_create(
    app: AppHandle,
    request: CreateCanvasRequest,
) -> Result<CanvasRecord, IpcError> {
    let svc = app.state::<CanvasDomainService>();
    let canvas = svc.create_canvas(
        &request.workspace_id,
        &request.name,
        request.description.as_deref(),
        request.conversation_ref.as_deref(),
    )?;
    Ok(canvas)
}

#[derive(serde::Deserialize)]
pub struct ListCanvasesRequest {
    pub workspace_id: String,
}

#[tauri::command]
pub async fn canvas_v1_list(
    app: AppHandle,
    request: ListCanvasesRequest,
) -> Result<Vec<CanvasRecord>, IpcError> {
    let svc = app.state::<CanvasDomainService>();
    let canvases = svc.list_canvases(&request.workspace_id)?;
    Ok(canvases)
}

#[derive(serde::Deserialize)]
pub struct GetCanvasRequest {
    pub id: String,
}

#[tauri::command]
pub async fn canvas_v1_get(
    app: AppHandle,
    request: GetCanvasRequest,
) -> Result<Option<CanvasRecord>, IpcError> {
    let svc = app.state::<CanvasDomainService>();
    let canvas = svc.get_canvas(&request.id)?;
    Ok(canvas)
}

#[tauri::command]
pub async fn canvas_v1_delete(app: AppHandle, request: GetCanvasRequest) -> Result<(), IpcError> {
    let svc = app.state::<CanvasDomainService>();
    svc.delete_canvas(&request.id)?;
    Ok(())
}

// ──────────────────────────────────────────────────────────────────
// Node CRUD
// ──────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct ListNodesRequest {
    pub canvas_id: String,
}

#[tauri::command]
pub async fn canvas_v1_list_nodes(
    app: AppHandle,
    request: ListNodesRequest,
) -> Result<Vec<CanvasNode>, IpcError> {
    let svc = app.state::<CanvasDomainService>();
    let nodes = svc.list_nodes(&request.canvas_id)?;
    Ok(nodes)
}

#[derive(serde::Deserialize)]
pub struct AddNodeRequest {
    pub canvas_id: String,
    pub kind: CanvasNodeKind,
    pub position: CanvasPosition,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub prompt: Option<String>,
}

#[tauri::command]
pub async fn canvas_v1_add_node(
    app: AppHandle,
    request: AddNodeRequest,
) -> Result<CanvasNode, IpcError> {
    let svc = app.state::<CanvasDomainService>();
    let draft = CanvasNodeDraft {
        canvas_id: request.canvas_id,
        kind: request.kind,
        position: request.position,
        size: None,
        summary: request.summary,
        description: request.description,
        prompt: request.prompt,
        status: Some(CanvasNodeStatus::Draft),
        refs: CanvasNodeRefs::default(),
        metadata: HashMap::new(),
    };
    let node = svc.add_node(draft)?;
    Ok(node)
}

#[derive(serde::Deserialize)]
pub struct UpdateNodeRequest {
    pub node_id: String,
    pub patch: NodePatch,
}

#[tauri::command]
pub async fn canvas_v1_update_node(
    app: AppHandle,
    request: UpdateNodeRequest,
) -> Result<CanvasNode, IpcError> {
    let svc = app.state::<CanvasDomainService>();
    let node = svc.update_node(&request.node_id, request.patch)?;
    Ok(node)
}

#[derive(serde::Deserialize)]
pub struct NodeIdRequest {
    pub node_id: String,
}

#[tauri::command]
pub async fn canvas_v1_delete_node(app: AppHandle, request: NodeIdRequest) -> Result<(), IpcError> {
    let svc = app.state::<CanvasDomainService>();
    svc.delete_node(&request.node_id)?;
    Ok(())
}

// ──────────────────────────────────────────────────────────────────
// Edge CRUD
// ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn canvas_v1_list_edges(
    app: AppHandle,
    request: ListNodesRequest,
) -> Result<Vec<CanvasEdge>, IpcError> {
    let svc = app.state::<CanvasDomainService>();
    let edges = svc.list_edges(&request.canvas_id)?;
    Ok(edges)
}

#[derive(serde::Deserialize)]
pub struct AddEdgeRequest {
    pub canvas_id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub kind: CanvasEdgeKind,
    pub label: Option<String>,
}

#[tauri::command]
pub async fn canvas_v1_add_edge(
    app: AppHandle,
    request: AddEdgeRequest,
) -> Result<CanvasEdge, IpcError> {
    let svc = app.state::<CanvasDomainService>();
    let draft = CanvasEdgeDraft {
        canvas_id: request.canvas_id,
        source_node_id: request.source_node_id,
        target_node_id: request.target_node_id,
        kind: request.kind,
        label: request.label,
        metadata: HashMap::new(),
    };
    let edge = svc.add_edge(draft)?;
    Ok(edge)
}

#[tauri::command]
pub async fn canvas_v1_delete_edge(app: AppHandle, request: NodeIdRequest) -> Result<(), IpcError> {
    let svc = app.state::<CanvasDomainService>();
    svc.delete_edge(&request.node_id)?;
    Ok(())
}
