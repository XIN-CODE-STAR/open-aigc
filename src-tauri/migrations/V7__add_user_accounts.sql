-- V7: 用户账户体系（扩展 teacher/student 表，管理员存 app_settings）
-- 教师账户字段（username + password_hash，均可空：无 username 表示未启用登录）
ALTER TABLE teacher ADD COLUMN username TEXT;
ALTER TABLE teacher ADD COLUMN password_hash TEXT;

CREATE UNIQUE INDEX ux_teacher_username
    ON teacher(username)
    WHERE username IS NOT NULL AND deleted_at IS NULL;

-- 学生账户字段
ALTER TABLE student ADD COLUMN username TEXT;
ALTER TABLE student ADD COLUMN password_hash TEXT;

CREATE UNIQUE INDEX ux_student_username
    ON student(username)
    WHERE username IS NOT NULL AND deleted_at IS NULL;

-- 管理员账户存储在 app_settings（admin_username / admin_password_hash）
-- 预置管理员账户在应用启动时由 AuthService::ensure_default_admin 写入。
