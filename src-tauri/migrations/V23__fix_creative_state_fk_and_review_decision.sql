-- V23: Fix creative_states FK target and allow needs_review review decisions.

DROP TABLE IF EXISTS creative_states_rebuild;

CREATE TABLE creative_states_rebuild (
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
    FOREIGN KEY (workspace_id) REFERENCES workspace(id)
);

INSERT INTO creative_states_rebuild (
    id, workspace_id, project_name, project_type, audience, platform,
    style_tokens_json, characters_json, scenes_json, references_json,
    constraints_json, history_decisions_json, version, created_at, updated_at
)
SELECT
    id, workspace_id, project_name, project_type, audience, platform,
    style_tokens_json, characters_json, scenes_json, references_json,
    constraints_json, history_decisions_json, version, created_at, updated_at
FROM creative_states;

DROP TABLE creative_states;
ALTER TABLE creative_states_rebuild RENAME TO creative_states;
CREATE INDEX IF NOT EXISTS idx_creative_states_workspace ON creative_states(workspace_id);

DROP TABLE IF EXISTS review_reports_rebuild;

CREATE TABLE review_reports_rebuild (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),

  project_id TEXT NOT NULL CHECK (length(trim(project_id)) BETWEEN 1 AND 36),
  run_id TEXT,
  shot_id TEXT,
  asset_id TEXT,
  generation_attempt_id TEXT,

  reviewer_type TEXT NOT NULL DEFAULT 'auto'
    CHECK (reviewer_type IN ('auto', 'manual', 'hybrid')),
  reviewer_agent_version TEXT,
  reviewer_provider TEXT,

  requirement_scores_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(requirement_scores_json)),
  visual_scores_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(visual_scores_json)),
  content_scores_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(content_scores_json)),
  commercial_scores_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(commercial_scores_json)),
  technical_scores_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(technical_scores_json)),

  overall_score REAL NOT NULL CHECK (overall_score BETWEEN 0 AND 100),
  weighted_score REAL CHECK (weighted_score BETWEEN 0 AND 100),
  issues_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(issues_json)),

  decision TEXT NOT NULL DEFAULT 'needs_review'
    CHECK (decision IN ('needs_review', 'accept', 'accept_with_suggestions', 'revise', 'regenerate', 'block')),

  confidence REAL CHECK (confidence BETWEEN 0 AND 1),
  source_task_id TEXT,
  review_version INTEGER NOT NULL DEFAULT 1 CHECK (review_version > 0),
  created_at TEXT NOT NULL
) STRICT;

INSERT INTO review_reports_rebuild (
  id, project_id, run_id, shot_id, asset_id, generation_attempt_id,
  reviewer_type, reviewer_agent_version, reviewer_provider,
  requirement_scores_json, visual_scores_json, content_scores_json,
  commercial_scores_json, technical_scores_json,
  overall_score, weighted_score, issues_json, decision, confidence,
  source_task_id, review_version, created_at
)
SELECT
  id, project_id, run_id, shot_id, asset_id, generation_attempt_id,
  reviewer_type, reviewer_agent_version, reviewer_provider,
  requirement_scores_json, visual_scores_json, content_scores_json,
  commercial_scores_json, technical_scores_json,
  overall_score, weighted_score, issues_json, decision, confidence,
  source_task_id, review_version, created_at
FROM review_reports;

DROP TABLE review_reports;
ALTER TABLE review_reports_rebuild RENAME TO review_reports;

CREATE INDEX ix_review_reports_project ON review_reports(project_id, created_at DESC);
CREATE INDEX ix_review_reports_shot ON review_reports(shot_id, overall_score DESC);
CREATE INDEX ix_review_reports_decision ON review_reports(decision)
  WHERE decision IN ('needs_review', 'revise', 'regenerate', 'block');
