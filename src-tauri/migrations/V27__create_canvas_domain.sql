-- V27: Canvas Domain — 画布领域模型（解耦自 Memory）
--
-- 新的画布领域表，使用强类型列替代 stringly-typed node_type/payload_json。
-- 与 V26 的 memory_canvases/memory_nodes/memory_edges 并行存在，
-- 迁移完成后旧表将被废弃。

-- 画布表
CREATE TABLE canvas_canvases (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    node_count INTEGER NOT NULL DEFAULT 0,
    edge_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);

CREATE INDEX idx_canvas_canvases_workspace ON canvas_canvases(workspace_id) WHERE deleted_at IS NULL;

-- 节点表 — 强类型 kind + typed refs
CREATE TABLE canvas_nodes (
    id TEXT PRIMARY KEY,
    canvas_id TEXT NOT NULL REFERENCES canvas_canvases(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,  -- CanvasNodeKind enum: scene/shot/image/video/upload/note/fact/document/character/prompt/agent-state
    position_x REAL NOT NULL DEFAULT 0.0,
    position_y REAL NOT NULL DEFAULT 0.0,
    width REAL,
    height REAL,
    summary TEXT,
    description TEXT,
    prompt TEXT,
    status TEXT,  -- CanvasNodeStatus enum: pending/generating/succeeded/failed/draft/ready
    -- Typed refs as individual nullable columns (queryable + indexable)
    memory_ref TEXT,
    artifact_ref TEXT,
    task_ref TEXT,
    shot_ref TEXT,
    scene_ref TEXT,
    project_ref TEXT,
    agent_ref TEXT,
    asset_ref TEXT,
    conversation_ref TEXT,
    -- Opaque metadata for extensibility
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);

CREATE INDEX idx_canvas_nodes_canvas ON canvas_nodes(canvas_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_canvas_nodes_kind ON canvas_nodes(kind) WHERE deleted_at IS NULL;
CREATE INDEX idx_canvas_nodes_shot_ref ON canvas_nodes(shot_ref) WHERE deleted_at IS NULL AND shot_ref IS NOT NULL;
CREATE INDEX idx_canvas_nodes_scene_ref ON canvas_nodes(scene_ref) WHERE deleted_at IS NULL AND scene_ref IS NOT NULL;
CREATE INDEX idx_canvas_nodes_project_ref ON canvas_nodes(project_ref) WHERE deleted_at IS NULL AND project_ref IS NOT NULL;
CREATE INDEX idx_canvas_nodes_task_ref ON canvas_nodes(task_ref) WHERE deleted_at IS NULL AND task_ref IS NOT NULL;
CREATE INDEX idx_canvas_nodes_artifact_ref ON canvas_nodes(artifact_ref) WHERE deleted_at IS NULL AND artifact_ref IS NOT NULL;
CREATE INDEX idx_canvas_nodes_conversation_ref ON canvas_nodes(conversation_ref) WHERE deleted_at IS NULL AND conversation_ref IS NOT NULL;

-- 边表 — 强类型 kind
CREATE TABLE canvas_edges (
    id TEXT PRIMARY KEY,
    canvas_id TEXT NOT NULL REFERENCES canvas_canvases(id) ON DELETE CASCADE,
    source_node_id TEXT NOT NULL REFERENCES canvas_nodes(id) ON DELETE CASCADE,
    target_node_id TEXT NOT NULL REFERENCES canvas_nodes(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,  -- CanvasEdgeKind enum: reference/dependency/sequence/composition/similarity/agent-link
    label TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    deleted_at TEXT
);

CREATE INDEX idx_canvas_edges_canvas ON canvas_edges(canvas_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_canvas_edges_source ON canvas_edges(source_node_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_canvas_edges_target ON canvas_edges(target_node_id) WHERE deleted_at IS NULL;
