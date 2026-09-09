-- V19: 创作工作流状态持久化
-- 存储工作流状态和事件历史，支持中断恢复

CREATE TABLE creative_workflows (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  project_id TEXT NOT NULL CHECK (length(trim(project_id)) BETWEEN 1 AND 36),
  asset_id TEXT NOT NULL CHECK (length(trim(asset_id)) BETWEEN 1 AND 36),
  shot_id TEXT,

  current_stage TEXT NOT NULL DEFAULT 'generating'
    CHECK (current_stage IN ('generating', 'evaluating', 'waiting_feedback', 'parsing_feedback',
      'planning_modification', 'applying_modification', 'regenerating', 'completed', 'failed')),

  iteration INTEGER NOT NULL DEFAULT 0 CHECK (iteration >= 0),
  max_iterations INTEGER NOT NULL DEFAULT 3 CHECK (max_iterations > 0),

  current_review_id TEXT,
  current_plan_id TEXT,

  is_paused INTEGER NOT NULL DEFAULT 0 CHECK (is_paused IN (0, 1)),
  error_message TEXT,

  config_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(config_json)),

  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_creative_workflows_project ON creative_workflows(project_id, updated_at DESC);
CREATE INDEX ix_creative_workflows_stage ON creative_workflows(current_stage) WHERE current_stage NOT IN ('completed', 'failed');

CREATE TABLE workflow_events (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  workflow_id TEXT NOT NULL CHECK (length(trim(workflow_id)) BETWEEN 1 AND 36),

  event_type TEXT NOT NULL,
  stage TEXT NOT NULL,
  detail TEXT,

  review_report_id TEXT,
  edit_request_id TEXT,
  edit_plan_id TEXT,

  created_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_workflow_events_workflow ON workflow_events(workflow_id, created_at DESC);
