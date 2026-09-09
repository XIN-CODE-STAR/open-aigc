-- V12: EverOS 记忆服务配置
-- 在 workspace 表添加 EverOS 配置 JSON 列

ALTER TABLE workspace ADD COLUMN everos_config_json TEXT;
