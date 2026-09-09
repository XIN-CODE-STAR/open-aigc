-- V17: AI Critic Agent 多维评价系统 + Edit Understanding Agent 结构化修改系统
-- 按照 2026-07-20-creative-agent-critical-capability-decisions.md 第 4-5 节实现
--
-- 变更概要：
--   1. 增强 review_reports：替换为 5 层评分体系
--   2. 新增 review_dimensions 表
--   3. 新增 edit_requests 表
--   4. 新增 edit_plans 表
--   5. 新增 asset_versions 表
--   6. 新增 asset_licenses 表
--   7. 新增 content_guard_reports 表
--
-- 注意：V17 新增表的外键约束故意延迟到 V16 迁移稳定后。
-- 跨表引用（platform_projects、platform_assets 等）暂不使用 FK，
-- 保留纯文本关联列；后续 schema 升级时统一收紧。

-- 1a. 创建新的 review_reports_v2 表
CREATE TABLE review_reports_v2 (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),

  -- 关联实体（纯文本 ID 列，不做 FK 校验）
  project_id TEXT NOT NULL CHECK (length(trim(project_id)) BETWEEN 1 AND 36),
  run_id TEXT,
  shot_id TEXT,
  asset_id TEXT,
  generation_attempt_id TEXT,

  -- Reviewer 元信息
  reviewer_type TEXT NOT NULL DEFAULT 'auto'
    CHECK (reviewer_type IN ('auto', 'manual', 'hybrid')),
  reviewer_agent_version TEXT,
  reviewer_provider TEXT,

  -- 五层评分
  requirement_scores_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(requirement_scores_json)),
  visual_scores_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(visual_scores_json)),
  content_scores_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(content_scores_json)),
  commercial_scores_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(commercial_scores_json)),
  technical_scores_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(technical_scores_json)),

  -- 汇总
  overall_score REAL NOT NULL CHECK (overall_score BETWEEN 0 AND 100),
  weighted_score REAL CHECK (weighted_score BETWEEN 0 AND 100),

  -- 问题列表
  issues_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(issues_json)),

  -- 评价决策
  decision TEXT NOT NULL DEFAULT 'needs_review'
    CHECK (decision IN ('accept', 'accept_with_suggestions', 'revise', 'regenerate', 'block')),

  confidence REAL CHECK (confidence BETWEEN 0 AND 1),

  source_task_id TEXT,

  review_version INTEGER NOT NULL DEFAULT 1 CHECK (review_version > 0),
  created_at TEXT NOT NULL
) STRICT;

-- 1b. 迁移旧数据
INSERT OR IGNORE INTO review_reports_v2 (
  id, project_id, shot_id, asset_id,
  reviewer_type,
  requirement_scores_json, visual_scores_json, content_scores_json,
  commercial_scores_json, technical_scores_json,
  overall_score, issues_json, decision, confidence,
  review_version, created_at
)
SELECT
  id, project_id, shot_id, asset_id,
  review_type AS reviewer_type,
  json_object('match', COALESCE(requirement_match_score, 0)) AS requirement_scores_json,
  json_object(
    'composition', COALESCE(composition_score, 0),
    'color_and_lighting', COALESCE(camera_score, 0)
  ) AS visual_scores_json,
  json_object('style_consistency', COALESCE(style_score, 0)) AS content_scores_json,
  '{}' AS commercial_scores_json,
  '{}' AS technical_scores_json,
  COALESCE(overall_score, 0) AS overall_score,
  COALESCE(issues_json, '[]') AS issues_json,
  CASE WHEN COALESCE(overall_score, 0) >= 85 THEN 'accept'
       WHEN COALESCE(overall_score, 0) >= 70 THEN 'accept_with_suggestions'
       WHEN COALESCE(overall_score, 0) >= 60 THEN 'revise'
       WHEN COALESCE(overall_score, 0) >= 40 THEN 'regenerate'
       ELSE 'block' END AS decision,
  NULL AS confidence,
  1 AS review_version,
  created_at
FROM review_reports;

-- 1c. 替换旧表
DROP TABLE review_reports;
ALTER TABLE review_reports_v2 RENAME TO review_reports;

-- 1d. 索引
CREATE INDEX ix_review_reports_project ON review_reports(project_id, created_at DESC);
CREATE INDEX ix_review_reports_shot ON review_reports(shot_id, overall_score DESC);
CREATE INDEX ix_review_reports_decision ON review_reports(decision) WHERE decision IN ('revise', 'regenerate', 'block');

-- ═══════════════════════════════════════════════════
-- 2. 评价维度
-- ═══════════════════════════════════════════════════

CREATE TABLE review_dimensions (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  review_id TEXT NOT NULL CHECK (length(trim(review_id)) BETWEEN 1 AND 36),

  dimension_layer TEXT NOT NULL
    CHECK (dimension_layer IN ('requirement', 'visual', 'content', 'commercial', 'technical')),
  dimension_name TEXT NOT NULL CHECK (length(trim(dimension_name)) BETWEEN 1 AND 60),

  score REAL NOT NULL CHECK (score BETWEEN 0 AND 100),
  weight REAL NOT NULL DEFAULT 1.0 CHECK (weight > 0 AND weight <= 5.0),
  confidence REAL CHECK (confidence BETWEEN 0 AND 1),

  reasoning TEXT,
  reference_context_json TEXT CHECK (json_valid(reference_context_json)),

  created_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_review_dimensions_review ON review_dimensions(review_id, dimension_layer);

-- ═══════════════════════════════════════════════════
-- 3. 编辑请求
-- ═══════════════════════════════════════════════════

CREATE TABLE edit_requests (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  project_id TEXT NOT NULL CHECK (length(trim(project_id)) BETWEEN 1 AND 36),
  run_id TEXT,

  feedback_text TEXT NOT NULL CHECK (length(trim(feedback_text)) BETWEEN 1 AND 2000),

  context_type TEXT NOT NULL DEFAULT 'generation_result'
    CHECK (context_type IN ('generation_result', 'storyboard', 'character_design', 'visual_spec', 'script', 'deliverable')),
  context_ref_id TEXT,

  source_review_id TEXT,

  intent_json TEXT CHECK (json_valid(intent_json)),

  status TEXT NOT NULL DEFAULT 'received'
    CHECK (status IN ('received', 'analyzing', 'plan_ready', 'applied', 'rejected', 'ambiguous')),
  ambiguous_reason TEXT,

  created_by TEXT NOT NULL,
  created_at TEXT NOT NULL,
  resolved_at TEXT
) STRICT;

CREATE INDEX ix_edit_requests_project ON edit_requests(project_id, created_at DESC);
CREATE INDEX ix_edit_requests_status ON edit_requests(status) WHERE status IN ('received', 'analyzing');

-- ═══════════════════════════════════════════════════
-- 4. 编辑计划
-- ═══════════════════════════════════════════════════

CREATE TABLE edit_plans (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  edit_request_id TEXT NOT NULL CHECK (length(trim(edit_request_id)) BETWEEN 1 AND 36),
  project_id TEXT NOT NULL CHECK (length(trim(project_id)) BETWEEN 1 AND 36),

  plan_summary TEXT NOT NULL CHECK (length(trim(plan_summary)) BETWEEN 1 AND 500),

  operation_type TEXT NOT NULL
    CHECK (operation_type IN (
      'style_adjustment', 'character_adjustment', 'composition_change',
      'lighting_adjustment', 'color_grading', 'camera_change',
      'content_revision', 'quality_improvement', 'regenerate'
    )),

  scope TEXT NOT NULL DEFAULT 'whole' CHECK (scope IN ('whole', 'partial')),

  targets_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(targets_json)),

  prompt_patch_json TEXT CHECK (json_valid(prompt_patch_json)),
  parameter_patch_json TEXT CHECK (json_valid(parameter_patch_json)),
  reference_asset_patch_json TEXT CHECK (json_valid(reference_asset_patch_json)),

  requires_regeneration INTEGER NOT NULL DEFAULT 0 CHECK (requires_regeneration IN (0, 1)),
  requires_critic_rerun INTEGER NOT NULL DEFAULT 1 CHECK (requires_critic_rerun IN (0, 1)),

  estimated_impact TEXT CHECK (estimated_impact IN ('low', 'medium', 'high')),
  risk_level TEXT CHECK (risk_level IN ('low', 'medium', 'high')),

  status TEXT NOT NULL DEFAULT 'draft'
    CHECK (status IN ('draft', 'ready', 'executing', 'executed', 'rejected')),

  execution_result_json TEXT CHECK (json_valid(execution_result_json)),

  created_by TEXT NOT NULL,
  created_at TEXT NOT NULL,
  executed_at TEXT
) STRICT;

CREATE INDEX ix_edit_plans_request ON edit_plans(edit_request_id, created_at DESC);
CREATE INDEX ix_edit_plans_project ON edit_plans(project_id, status);

-- ═══════════════════════════════════════════════════
-- 5. 资产版本
-- ═══════════════════════════════════════════════════

CREATE TABLE asset_versions (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  asset_id TEXT NOT NULL CHECK (length(trim(asset_id)) BETWEEN 1 AND 36),

  version INTEGER NOT NULL CHECK (version > 0),

  storage_key TEXT NOT NULL,
  mime_type TEXT NOT NULL,
  size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
  hash TEXT,
  width INTEGER,
  height INTEGER,
  duration_seconds REAL,

  source_type TEXT NOT NULL CHECK (source_type IN ('generated', 'uploaded', 'edited', 'imported')),
  source_task_id TEXT,
  source_attempt_id TEXT,

  status TEXT NOT NULL DEFAULT 'draft'
    CHECK (status IN (
      'draft', 'reviewing', 'approved', 'selected', 'used_in_deliverable',
      'archived', 'deprecated', 'rejected'
    )),
  status_reason TEXT,

  review_id TEXT,

  created_by TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,

  UNIQUE (asset_id, version)
) STRICT;

CREATE INDEX ix_asset_versions_asset ON asset_versions(asset_id, version DESC);

-- ═══════════════════════════════════════════════════
-- 6. 资产版权
-- ═══════════════════════════════════════════════════

CREATE TABLE asset_licenses (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  asset_id TEXT NOT NULL CHECK (length(trim(asset_id)) BETWEEN 1 AND 36),
  asset_version_id TEXT,

  source_type TEXT NOT NULL CHECK (source_type IN ('ai_generated', 'user_uploaded', 'third_party', 'derived')),

  provider_id TEXT,
  model_name TEXT,

  commercial_use_status TEXT NOT NULL DEFAULT 'unknown'
    CHECK (commercial_use_status IN ('clear', 'needs_review', 'restricted', 'blocked', 'unknown')),
  commercial_use_details TEXT,

  source_assets_json TEXT CHECK (json_valid(source_assets_json)),

  copyright_statement TEXT,
  attribution_required INTEGER NOT NULL DEFAULT 0 CHECK (attribution_required IN (0, 1)),

  risk_flags_json TEXT CHECK (json_valid(risk_flags_json)),

  review_required INTEGER NOT NULL DEFAULT 0 CHECK (review_required IN (0, 1)),
  review_notes TEXT,

  export_allowed INTEGER NOT NULL DEFAULT 0 CHECK (export_allowed IN (0, 1)),
  export_block_reason TEXT,

  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_asset_licenses_asset ON asset_licenses(asset_id);

-- ═══════════════════════════════════════════════════
-- 7. 内容安全
-- ═══════════════════════════════════════════════════

CREATE TABLE content_guard_reports (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),

  project_id TEXT NOT NULL CHECK (length(trim(project_id)) BETWEEN 1 AND 36),
  target_type TEXT NOT NULL CHECK (target_type IN ('user_prompt', 'generated_prompt', 'asset', 'deliverable', 'script')),
  target_id TEXT,

  task_id TEXT,
  asset_id TEXT,

  guard_version TEXT,
  guard_provider TEXT,

  status TEXT NOT NULL DEFAULT 'passed'
    CHECK (status IN ('passed', 'needs_review', 'flagged', 'blocked')),
  risk_level TEXT NOT NULL DEFAULT 'low'
    CHECK (risk_level IN ('low', 'medium', 'high', 'critical')),

  checks_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(checks_json)),
  actions_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(actions_json)),

  created_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_content_guard_reports_project ON content_guard_reports(project_id, created_at DESC);
