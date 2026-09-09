use tauri::{AppHandle, Manager};

use crate::{
    application::memory_canvas_service::MemoryCanvasService,
    ports::memory_canvas_repository::{MemoryCanvas, MemoryEdge, MemoryNode, MemoryViewport},
};

use super::error::IpcError;

// ──────────────────────────────────────────────────────────────────
// Canvas commands
// ──────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct CreateCanvasRequest {
    pub workspace_id: String,
    pub name: String,
    pub description: Option<String>,
}

#[tauri::command]
pub async fn memory_canvas_v1_create(
    app: AppHandle,
    request: CreateCanvasRequest,
) -> Result<MemoryCanvas, IpcError> {
    let service = app.state::<MemoryCanvasService>();
    let canvas = service.create_canvas(
        &request.workspace_id,
        &request.name,
        request.description.as_deref(),
    )?;
    Ok(canvas)
}

#[derive(serde::Deserialize)]
pub struct ListCanvasesRequest {
    pub workspace_id: String,
}

#[tauri::command]
pub async fn memory_canvas_v1_list(
    app: AppHandle,
    request: ListCanvasesRequest,
) -> Result<Vec<MemoryCanvas>, IpcError> {
    let service = app.state::<MemoryCanvasService>();
    let canvases = service.list_canvases(&request.workspace_id)?;
    Ok(canvases)
}

#[derive(serde::Deserialize)]
pub struct GetCanvasRequest {
    pub id: String,
}

#[tauri::command]
pub async fn memory_canvas_v1_get(
    app: AppHandle,
    request: GetCanvasRequest,
) -> Result<Option<MemoryCanvas>, IpcError> {
    let service = app.state::<MemoryCanvasService>();
    let canvas = service.get_canvas(&request.id)?;
    Ok(canvas)
}

#[derive(serde::Deserialize)]
pub struct DeleteCanvasRequest {
    pub id: String,
}

#[tauri::command]
pub async fn memory_canvas_v1_delete(
    app: AppHandle,
    request: DeleteCanvasRequest,
) -> Result<(), IpcError> {
    let service = app.state::<MemoryCanvasService>();
    service.delete_canvas(&request.id)?;
    Ok(())
}

// ──────────────────────────────────────────────────────────────────
// Node commands
// ──────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct AddNodeRequest {
    pub canvas_id: String,
    pub node_type: String,
    pub position_x: f64,
    pub position_y: f64,
    pub payload_json: String,
    pub summary: Option<String>,
    pub asset_id: Option<String>,
}

#[tauri::command]
pub async fn memory_node_v1_add(
    app: AppHandle,
    request: AddNodeRequest,
) -> Result<MemoryNode, IpcError> {
    let service = app.state::<MemoryCanvasService>();
    let node = service.add_node(
        &request.canvas_id,
        &request.node_type,
        request.position_x,
        request.position_y,
        &request.payload_json,
        request.summary.as_deref(),
        request.asset_id.as_deref(),
    )?;
    Ok(node)
}

#[derive(serde::Deserialize)]
pub struct UpdateNodeRequest {
    pub id: String,
    pub canvas_id: String,
    pub node_type: String,
    pub position_x: f64,
    pub position_y: f64,
    pub payload_json: String,
    pub summary: Option<String>,
    pub asset_id: Option<String>,
}

#[tauri::command]
pub async fn memory_node_v1_update(
    app: AppHandle,
    request: UpdateNodeRequest,
) -> Result<MemoryNode, IpcError> {
    let service = app.state::<MemoryCanvasService>();
    let node = service.update_node(
        &request.id,
        &request.canvas_id,
        &request.node_type,
        request.position_x,
        request.position_y,
        &request.payload_json,
        request.summary.as_deref(),
        request.asset_id.as_deref(),
    )?;
    Ok(node)
}

#[derive(serde::Deserialize)]
pub struct DeleteNodeRequest {
    pub id: String,
}

#[tauri::command]
pub async fn memory_node_v1_delete(
    app: AppHandle,
    request: DeleteNodeRequest,
) -> Result<(), IpcError> {
    let service = app.state::<MemoryCanvasService>();
    service.delete_node(&request.id)?;
    Ok(())
}

#[derive(serde::Deserialize)]
pub struct ListNodesRequest {
    pub canvas_id: String,
}

#[tauri::command]
pub async fn memory_node_v1_list(
    app: AppHandle,
    request: ListNodesRequest,
) -> Result<Vec<MemoryNode>, IpcError> {
    let service = app.state::<MemoryCanvasService>();
    let nodes = service.list_nodes(&request.canvas_id)?;
    Ok(nodes)
}

// ──────────────────────────────────────────────────────────────────
// Edge commands
// ──────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct AddEdgeRequest {
    pub canvas_id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub edge_type: String,
    pub label: Option<String>,
}

#[tauri::command]
pub async fn memory_edge_v1_add(
    app: AppHandle,
    request: AddEdgeRequest,
) -> Result<MemoryEdge, IpcError> {
    let service = app.state::<MemoryCanvasService>();
    let edge = service.add_edge(
        &request.canvas_id,
        &request.source_node_id,
        &request.target_node_id,
        &request.edge_type,
        request.label.as_deref(),
    )?;
    Ok(edge)
}

#[derive(serde::Deserialize)]
pub struct DeleteEdgeRequest {
    pub id: String,
}

#[tauri::command]
pub async fn memory_edge_v1_delete(
    app: AppHandle,
    request: DeleteEdgeRequest,
) -> Result<(), IpcError> {
    let service = app.state::<MemoryCanvasService>();
    service.delete_edge(&request.id)?;
    Ok(())
}

#[derive(serde::Deserialize)]
pub struct ListEdgesRequest {
    pub canvas_id: String,
}

#[tauri::command]
pub async fn memory_edge_v1_list(
    app: AppHandle,
    request: ListEdgesRequest,
) -> Result<Vec<MemoryEdge>, IpcError> {
    let service = app.state::<MemoryCanvasService>();
    let edges = service.list_edges(&request.canvas_id)?;
    Ok(edges)
}

// ──────────────────────────────────────────────────────────────────
// Viewport commands
// ──────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct SaveViewportRequest {
    pub canvas_id: String,
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
}

#[tauri::command]
pub async fn memory_viewport_v1_save(
    app: AppHandle,
    request: SaveViewportRequest,
) -> Result<(), IpcError> {
    let service = app.state::<MemoryCanvasService>();
    service.save_viewport(
        &request.canvas_id,
        request.zoom,
        request.pan_x,
        request.pan_y,
    )?;
    Ok(())
}

#[derive(serde::Deserialize)]
pub struct GetViewportRequest {
    pub canvas_id: String,
}

#[tauri::command]
pub async fn memory_viewport_v1_get(
    app: AppHandle,
    request: GetViewportRequest,
) -> Result<MemoryViewport, IpcError> {
    let service = app.state::<MemoryCanvasService>();
    let viewport = service.get_viewport(&request.canvas_id)?;
    Ok(viewport)
}
