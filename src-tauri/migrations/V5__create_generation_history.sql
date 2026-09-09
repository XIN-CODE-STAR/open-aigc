-- Provider 输出接入：AI 生成结果必须先落入受管文件流程（asset_manifest），再进入生成历史。
-- generation_tasks 记录生成请求；generation_results 通过 FK 引用 asset_manifest，确保产物先入 manifest 再入历史。
CREATE TABLE generation_tasks (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  workspace_id TEXT NOT NULL REFERENCES workspace(id),
  provider_name TEXT NOT NULL CHECK (length(trim(provider_name)) BETWEEN 1 AND 80),
  model_name TEXT NOT NULL CHECK (length(trim(model_name)) BETWEEN 1 AND 120),
  prompt_text TEXT NOT NULL CHECK (length(prompt_text) BETWEEN 1 AND 4000),
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'running', 'succeeded', 'failed')),
  error_message TEXT,
  parameters_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(parameters_json)),
  revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  started_at TEXT,
  completed_at TEXT
) STRICT;

CREATE INDEX ix_generation_tasks_status
  ON generation_tasks(status, created_at DESC);

CREATE INDEX ix_generation_tasks_provider
  ON generation_tasks(provider_name, created_at DESC);

-- 生成结果：每条记录必须引用 asset_manifest 中已存在的资产。
-- 这确保 AI 输出必须先经过受管文件导入流程，才能进入生成历史。
CREATE TABLE generation_results (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  task_id TEXT NOT NULL REFERENCES generation_tasks(id),
  asset_id TEXT NOT NULL REFERENCES asset_manifest(id),
  created_at TEXT NOT NULL
) STRICT;

CREATE UNIQUE INDEX ux_generation_results_task_asset
  ON generation_results(task_id, asset_id);

CREATE INDEX ix_generation_results_task
  ON generation_results(task_id, created_at DESC);

CREATE INDEX ix_generation_results_asset
  ON generation_results(asset_id);
