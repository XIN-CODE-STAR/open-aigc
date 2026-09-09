-- Phase A: Credential Type System
-- 为 provider_credentials 表添加认证类型和作用域字段。
-- 默认值兼容现有数据（全部为 api_key + user）。

ALTER TABLE provider_credentials ADD COLUMN credential_type TEXT NOT NULL DEFAULT 'api_key';
ALTER TABLE provider_credentials ADD COLUMN scope TEXT NOT NULL DEFAULT 'user';
