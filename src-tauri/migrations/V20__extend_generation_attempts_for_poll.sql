-- V20: 扩展 generation_attempts 表，支持 PollWorker 轮询
-- 按照 2026-07-21-creative-workshop-agent-implementation-orchestration.md 步骤二实现
--
-- 注意：capability 字段已存在于 V15，只添加缺失的字段。

-- 新增字段：provider_id、consecutive_failures、last_poll_at
ALTER TABLE generation_attempts ADD COLUMN provider_id TEXT NOT NULL DEFAULT '';
ALTER TABLE generation_attempts ADD COLUMN consecutive_failures INTEGER NOT NULL DEFAULT 0;
ALTER TABLE generation_attempts ADD COLUMN last_poll_at TEXT;

-- 创建活跃任务轮询索引
CREATE INDEX IF NOT EXISTS idx_attempts_active_poll
ON generation_attempts(status, capability)
WHERE status IN ('submitted', 'polling');
