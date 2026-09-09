-- V26: 记忆画布系统
-- 无限画布 + 节点/连线/视口持久化，用于 Agent 记忆可视化与 RAG 检索。
--
-- 设计原则：
--   1. 画布与节点分离：一个画布包含多个节点，节点可绑定资产
--   2. 节点类型多样：事实、笔记、图片、视频、文档
--   3. 支持 RAG：节点 summary 字段用于向量检索
--   4. 与资源库复用：通过 asset_id 关联 asset_manifest

-- ═══════════════════════════════════════════════════
-- 1. 记忆画布表
-- ═══════════════════════════════════════════════════

CREATE TABLE memory_canvases (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  workspace_id TEXT NOT NULL,
  name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 200),
  description TEXT,
  node_count INTEGER NOT NULL DEFAULT 0 CHECK (node_count >= 0),
  edge_count INTEGER NOT NULL DEFAULT 0 CHECK (edge_count >= 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  deleted_at TEXT
) STRICT;

CREATE INDEX ix_memory_canvases_workspace ON memory_canvases(workspace_id) WHERE deleted_at IS NULL;

-- ═══════════════════════════════════════════════════
-- 2. 记忆节点表
-- ═══════════════════════════════════════════════════

CREATE TABLE memory_nodes (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  canvas_id TEXT NOT NULL REFERENCES memory_canvases(id) ON DELETE CASCADE,

  -- 节点类型
  node_type TEXT NOT NULL CHECK (node_type IN (
    'fact', 'note', 'image', 'video', 'document'
  )),

  -- 画布位置
  position_x REAL NOT NULL DEFAULT 0,
  position_y REAL NOT NULL DEFAULT 0,
  width REAL,
  height REAL,

  -- 节点内容（结构化 JSON）
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),

  -- 摘要（供 RAG/Agent 检索）
  summary TEXT CHECK (length(summary) <= 500),

  -- 关联资产
  asset_id TEXT REFERENCES asset_manifest(id) ON DELETE SET NULL,

  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  deleted_at TEXT
) STRICT;

CREATE INDEX ix_memory_nodes_canvas ON memory_nodes(canvas_id) WHERE deleted_at IS NULL;
CREATE INDEX ix_memory_nodes_asset ON memory_nodes(asset_id) WHERE asset_id IS NOT NULL AND deleted_at IS NULL;

-- ═══════════════════════════════════════════════════
-- 3. 记忆连线表
-- ═══════════════════════════════════════════════════

CREATE TABLE memory_edges (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  canvas_id TEXT NOT NULL REFERENCES memory_canvases(id) ON DELETE CASCADE,
  source_node_id TEXT NOT NULL REFERENCES memory_nodes(id) ON DELETE CASCADE,
  target_node_id TEXT NOT NULL REFERENCES memory_nodes(id) ON DELETE CASCADE,
  edge_type TEXT NOT NULL DEFAULT 'link' CHECK (edge_type IN ('link', 'dependency', 'reference', 'similarity')),
  label TEXT,
  created_at TEXT NOT NULL,
  deleted_at TEXT
) STRICT;

CREATE INDEX ix_memory_edges_canvas ON memory_edges(canvas_id) WHERE deleted_at IS NULL;
CREATE INDEX ix_memory_edges_source ON memory_edges(source_node_id) WHERE deleted_at IS NULL;
CREATE INDEX ix_memory_edges_target ON memory_edges(target_node_id) WHERE deleted_at IS NULL;

-- ═══════════════════════════════════════════════════
-- 4. 画布视口持久化表
-- ═══════════════════════════════════════════════════

CREATE TABLE memory_canvas_viewports (
  canvas_id TEXT PRIMARY KEY REFERENCES memory_canvases(id) ON DELETE CASCADE,
  zoom REAL NOT NULL DEFAULT 1.0 CHECK (zoom > 0),
  pan_x REAL NOT NULL DEFAULT 0,
  pan_y REAL NOT NULL DEFAULT 0,
  updated_at TEXT NOT NULL
) STRICT;
