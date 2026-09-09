-- V6: Provider 凭据存储 + 管理员设置 + 生成任务进度字段
-- 凭据密钥本身通过 OS keychain 存储，此表只保存元信息和 keychain 引用键。

CREATE TABLE provider_credentials (
    id TEXT PRIMARY KEY NOT NULL,
    provider_name TEXT NOT NULL,
    display_name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    model_name TEXT NOT NULL,
    credential_key TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);

CREATE INDEX ix_provider_credentials_name
    ON provider_credentials(provider_name)
    WHERE deleted_at IS NULL;

CREATE INDEX ix_provider_credentials_enabled
    ON provider_credentials(enabled)
    WHERE deleted_at IS NULL;

-- 应用级键值设置表，用于管理员密码哈希等。
CREATE TABLE app_settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- 生成任务进度字段（0-100 整数），用于 SSE/事件推送。
ALTER TABLE generation_tasks ADD COLUMN progress INTEGER NOT NULL DEFAULT 0
    CHECK (progress >= 0 AND progress <= 100);
