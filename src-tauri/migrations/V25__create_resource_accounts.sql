-- V25: 用户 AI 资源账号表。
-- 与 provider_credentials 并存：provider_credentials 存 API Key 类凭据，
-- resource_accounts 存用户账号登录态（session/cookie/browser）。

CREATE TABLE IF NOT EXISTS resource_accounts (
    id TEXT PRIMARY KEY NOT NULL,
    provider_id TEXT NOT NULL,
    account_type TEXT NOT NULL DEFAULT 'session_cookie',
    display_name TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'active',
    credential_key TEXT NOT NULL,
    base_url TEXT NOT NULL DEFAULT '',
    extra_json TEXT NOT NULL DEFAULT '{}',
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_health_check_at TEXT,
    deleted_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_resource_accounts_provider
    ON resource_accounts (provider_id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_resource_accounts_status
    ON resource_accounts (status)
    WHERE deleted_at IS NULL AND enabled = 1;
