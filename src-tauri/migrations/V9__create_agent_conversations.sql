-- Agent 模式：Grok agent 工具循环持久化
-- 设计参考 Grok 应用的 agent 工作流：多轮对话 + function calling + 工具调用记录
-- 与 generation_tasks 分离：generation_tasks 是单次生成任务，agent_conversations 是多轮推理会话
-- agent_messages 包含 role（system/user/assistant/tool）与 OpenAI Chat Completions 协议一致
-- agent_tool_invocations 记录每次工具调用的入参/结果/状态，便于审计和回放

CREATE TABLE agent_conversations (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  workspace_id TEXT NOT NULL REFERENCES workspace(id),
  title TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 200),
  -- 关联的 chat LLM 凭据（必须是支持 chat completions 的供应商，如 xAI）
  credential_id TEXT NOT NULL REFERENCES provider_credentials(id),
  system_prompt TEXT,
  status TEXT NOT NULL DEFAULT 'active'
    CHECK (status IN ('active', 'archived', 'error')),
  revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_agent_conversations_workspace
  ON agent_conversations(workspace_id, updated_at DESC);

-- 消息表：完整记录 OpenAI Chat Completions messages 协议
-- role: system / user / assistant / tool
-- assistant 消息可携带 tool_calls（JSON 数组）
-- tool 消息必须引用 tool_call_id
CREATE TABLE agent_messages (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  conversation_id TEXT NOT NULL REFERENCES agent_conversations(id),
  role TEXT NOT NULL CHECK (role IN ('system', 'user', 'assistant', 'tool')),
  -- 文本内容（assistant 消息可能为空，仅返回 tool_calls）
  content TEXT,
  -- assistant 消息的 tool_calls（JSON 数组，OpenAI 风格）
  -- 结构：[{id, type:'function', function:{name, arguments}}]
  tool_calls_json TEXT,
  -- tool 消息对应的 tool_call_id（role='tool' 时必填）
  tool_call_id TEXT,
  -- 关联的 LLM 远程调用标识（assistant 消息），便于追踪
  remote_model TEXT,
  -- 完成 reason：stop / tool_calls / length / content_filter
  finish_reason TEXT,
  -- 父消息 ID，用于构建消息树（多分支对话扩展）
  parent_message_id TEXT REFERENCES agent_messages(id),
  revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
  created_at TEXT NOT NULL,
  -- token 用量估算（仅 assistant 消息）
  prompt_tokens INTEGER,
  completion_tokens INTEGER
) STRICT;

CREATE INDEX ix_agent_messages_conversation
  ON agent_messages(conversation_id, created_at ASC);

CREATE INDEX ix_agent_messages_parent
  ON agent_messages(parent_message_id);

-- 工具调用记录：每次 LLM 决定调用工具的完整审计轨迹
-- 与 agent_messages.tool_calls_json 一一对应，但独立存储便于查询/重试
CREATE TABLE agent_tool_invocations (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  message_id TEXT NOT NULL REFERENCES agent_messages(id),
  conversation_id TEXT NOT NULL REFERENCES agent_conversations(id),
  -- 工具名称（image_generation / video_generation / list_credentials / current_time 等）
  tool_name TEXT NOT NULL CHECK (length(trim(tool_name)) BETWEEN 1 AND 80),
  -- 工具入参（JSON 对象）
  arguments_json TEXT NOT NULL CHECK (json_valid(arguments_json)),
  -- 工具执行结果（JSON 值，失败时可为空）
  result_json TEXT,
  -- 执行状态
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'running', 'succeeded', 'failed', 'skipped')),
  error_message TEXT,
  -- 若工具触发了 generation_task，记录关联 ID
  generation_task_id TEXT REFERENCES generation_tasks(id),
  started_at TEXT,
  completed_at TEXT,
  created_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_agent_tool_invocations_message
  ON agent_tool_invocations(message_id);

CREATE INDEX ix_agent_tool_invocations_conversation
  ON agent_tool_invocations(conversation_id, created_at ASC);

CREATE INDEX ix_agent_tool_invocations_status
  ON agent_tool_invocations(status);
