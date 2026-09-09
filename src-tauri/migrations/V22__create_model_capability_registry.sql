-- V22: Capability Registry for Cognitive Router
--
-- Phase 1 stores model capability vectors in SQLite. Adding a new model should
-- be data insertion rather than Rust code changes.

CREATE TABLE model_capabilities (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  provider_id TEXT NOT NULL CHECK (length(trim(provider_id)) BETWEEN 1 AND 80),
  model_name TEXT NOT NULL CHECK (length(trim(model_name)) BETWEEN 1 AND 120),
  display_name TEXT NOT NULL CHECK (length(trim(display_name)) BETWEEN 1 AND 160),
  task_type TEXT NOT NULL CHECK (task_type IN (
    'text_generation',
    'image_generation',
    'video_generation',
    'text_to_speech',
    'vision_evaluation',
    'content_guard',
    'feedback_parsing',
    'character_consistency',
    'style_consistency'
  )),

  capabilities_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(capabilities_json)),
  prerequisites_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(prerequisites_json)),
  constraints_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(constraints_json)),
  failure_log_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(failure_log_json)),

  cost_score REAL NOT NULL DEFAULT 0.5 CHECK (cost_score >= 0.0 AND cost_score <= 1.0),
  speed_score REAL NOT NULL DEFAULT 0.5 CHECK (speed_score >= 0.0 AND speed_score <= 1.0),
  quality_score REAL NOT NULL DEFAULT 0.5 CHECK (quality_score >= 0.0 AND quality_score <= 1.0),

  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,

  UNIQUE(provider_id, model_name, task_type)
) STRICT;

CREATE INDEX ix_model_capabilities_task_enabled
  ON model_capabilities(task_type, enabled);

CREATE INDEX ix_model_capabilities_provider_model
  ON model_capabilities(provider_id, model_name);

INSERT INTO model_capabilities (
  id, provider_id, model_name, display_name, task_type,
  capabilities_json, prerequisites_json, constraints_json, failure_log_json,
  cost_score, speed_score, quality_score, enabled, created_at, updated_at
) VALUES
(
  '11111111-1111-4111-8111-111111111111',
  'claude',
  'claude-sonnet-4',
  'Claude / Creative Director',
  'text_generation',
  json_object(
    'planning', 0.96,
    'scriptwriting', 0.94,
    'visual_reasoning', 0.88,
    'prompt_adherence', 0.92,
    'image_quality', 0.10,
    'camera_motion', 0.10,
    'character_consistency', 0.72,
    'cost_efficiency', 0.55
  ),
  json_array(),
  json_object('supportsReference', json('true'), 'supportedRatios', json_array()),
  json_object(),
  0.55,
  0.72,
  0.94,
  1,
  datetime('now'),
  datetime('now')
),
(
  '22222222-2222-4222-8222-222222222222',
  'openai',
  'omni-image',
  'Omni / Storyboard Image',
  'image_generation',
  json_object(
    'planning', 0.25,
    'scriptwriting', 0.20,
    'visual_reasoning', 0.75,
    'prompt_adherence', 0.90,
    'image_quality', 0.92,
    'camera_motion', 0.15,
    'character_consistency', 0.72,
    'cost_efficiency', 0.62
  ),
  json_array(),
  json_object(
    'supportsReference', json('true'),
    'maxResolution', '2048x2048',
    'supportedRatios', json_array('16:9', '9:16', '1:1')
  ),
  json_object(),
  0.62,
  0.68,
  0.92,
  1,
  datetime('now'),
  datetime('now')
),
(
  '33333333-3333-4333-8333-333333333333',
  'seedance',
  'seedance-v2',
  'Seedance / Video Clip',
  'video_generation',
  json_object(
    'planning', 0.18,
    'scriptwriting', 0.15,
    'visual_reasoning', 0.62,
    'prompt_adherence', 0.78,
    'image_quality', 0.78,
    'camera_motion', 0.90,
    'character_consistency', 0.70,
    'cost_efficiency', 0.64
  ),
  json_array('character_consistency -> 需先有角色参考图或分镜图'),
  json_object(
    'supportsReference', json('true'),
    'maxDurationSeconds', 10,
    'maxResolution', '1920x1080',
    'supportedRatios', json_array('16:9', '9:16')
  ),
  json_object(),
  0.64,
  0.48,
  0.86,
  1,
  datetime('now'),
  datetime('now')
);
