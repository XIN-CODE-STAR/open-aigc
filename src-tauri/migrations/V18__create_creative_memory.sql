-- V18: User Creative Memory 系统
-- 用户创意记忆：偏好学习、风格沉淀、工作流习惯记录
--
-- 设计原则：
--   1. 只保存用户明确确认的偏好（不自动写入推测结果）
--   2. 支持查看、编辑、删除、暂停记忆
--   3. 记忆有作用域：user（全局）、project（项目）、brand（品牌）
--   4. 学生隐私和客户敏感资料不进入记忆

-- ═══════════════════════════════════════════════════
-- 1. 创意记忆表
-- ═══════════════════════════════════════════════════

CREATE TABLE creative_memory (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),

  -- 记忆类型
  memory_type TEXT NOT NULL CHECK (memory_type IN (
    'style_preference',
    'negative_preference',
    'brand_rule',
    'workflow_habit',
    'prompt_pattern',
    'review_history'
  )),

  -- 作用域
  scope TEXT NOT NULL CHECK (scope IN ('user', 'project', 'brand')),
  scope_ref_id TEXT,  -- project_id 或 brand_id（scope=user 时为 NULL）

  -- 记忆内容（结构化 JSON）
  -- style_preference: { "color_palette": [...], "composition": "...", "lighting": "..." }
  -- negative_preference: { "avoid": ["...", "..."] }
  -- brand_rule: { "brand_colors": [...], "logo_rules": "...", "banned_words": [...] }
  -- workflow_habit: { "step": "...", "frequency": 0.8 }
  -- prompt_pattern: { "template": "...", "success_rate": 0.9, "use_count": 5 }
  -- review_history: { "action": "accepted", "modification_type": "...", "count": 3 }
  content_json TEXT NOT NULL CHECK (json_valid(content_json)),

  -- 人类可读的摘要
  summary TEXT NOT NULL CHECK (length(trim(summary)) BETWEEN 1 AND 500),

  -- 来源追溯
  source TEXT NOT NULL CHECK (source IN ('explicit_save', 'confirmed_pattern', 'project_template')),
  source_ref_id TEXT,  -- 来源的引用（如 edit_request_id、project_id）

  -- 状态
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'paused', 'archived')),

  -- 置信度（用户确认次数越多越高）
  confidence REAL NOT NULL DEFAULT 0.5 CHECK (confidence BETWEEN 0 AND 1),
  confirm_count INTEGER NOT NULL DEFAULT 1 CHECK (confirm_count >= 0),

  -- 审计追踪
  created_by TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_creative_memory_scope ON creative_memory(scope, scope_ref_id) WHERE status = 'active';
CREATE INDEX ix_creative_memory_type ON creative_memory(memory_type, scope) WHERE status = 'active';
CREATE INDEX ix_creative_memory_user ON creative_memory(created_by, memory_type) WHERE status = 'active';

-- ═══════════════════════════════════════════════════
-- 2. 创意记忆事件表（审计日志）
-- ═══════════════════════════════════════════════════

CREATE TABLE creative_memory_events (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  memory_id TEXT NOT NULL CHECK (length(trim(memory_id)) BETWEEN 1 AND 36),
  event_type TEXT NOT NULL CHECK (event_type IN ('created', 'updated', 'paused', 'resumed', 'archived', 'deleted', 'confirmed')),
  event_detail TEXT,
  created_by TEXT NOT NULL,
  created_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_creative_memory_events_memory ON creative_memory_events(memory_id, created_at DESC);
