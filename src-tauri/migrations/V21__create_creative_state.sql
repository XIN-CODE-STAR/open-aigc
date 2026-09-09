-- V21: 创建创作状态表（Creative State）
-- 项目级持久化创作上下文，所有 Agent 执行时读取，生成后更新。

CREATE TABLE IF NOT EXISTS creative_states (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    project_name TEXT NOT NULL DEFAULT '',
    project_type TEXT NOT NULL DEFAULT '',
    audience TEXT NOT NULL DEFAULT '',
    platform TEXT NOT NULL DEFAULT '',
    style_tokens_json TEXT NOT NULL DEFAULT '{}',
    characters_json TEXT NOT NULL DEFAULT '[]',
    scenes_json TEXT NOT NULL DEFAULT '[]',
    references_json TEXT NOT NULL DEFAULT '[]',
    constraints_json TEXT NOT NULL DEFAULT '[]',
    history_decisions_json TEXT NOT NULL DEFAULT '[]',
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id)
);

CREATE INDEX IF NOT EXISTS idx_creative_states_workspace ON creative_states(workspace_id);
