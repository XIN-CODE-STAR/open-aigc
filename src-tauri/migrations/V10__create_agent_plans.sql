-- V10: Agent Plan-and-Execute 架构
-- 创建 agent_plans 表用于持久化执行计划
-- 在 agent_conversations 添加新列用于中断/恢复和记忆配置

CREATE TABLE agent_plans (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  conversation_id TEXT NOT NULL REFERENCES agent_conversations(id) ON DELETE CASCADE,
  goal TEXT NOT NULL CHECK (length(trim(goal)) BETWEEN 1 AND 2000),
  -- JSON 数组: [{index, description, status}]
  steps_json TEXT NOT NULL CHECK (json_valid(steps_json)),
  -- 整体状态: pending / in-progress / completed / failed / skipped
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'in-progress', 'completed', 'failed', 'skipped')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_agent_plans_conversation
  ON agent_plans(conversation_id, created_at DESC);

-- Agent 循环状态（用于 ask_user_question 中断/恢复）
ALTER TABLE agent_conversations ADD COLUMN loop_state_json TEXT;

-- 执行模式: plan-and-execute (默认) / react
ALTER TABLE agent_conversations ADD COLUMN execution_mode TEXT NOT NULL DEFAULT 'plan-and-execute'
  CHECK (execution_mode IN ('plan-and-execute', 'react'));

-- 记忆配置
ALTER TABLE agent_conversations ADD COLUMN memory_config_json TEXT;
