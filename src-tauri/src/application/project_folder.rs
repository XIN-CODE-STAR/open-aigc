//! 项目工作目录内的 `.openaigc` 项目文件夹。
//!
//! 对标主流 Agent 的惯例（Claude Code 的 `.claude/`、Codex 的 `.codex/`、
//! Trae 的 `.trae/`）：在用户打开的工作目录里落地项目级状态，让
//! "再次打开同一个目录"能恢复一致的上下文：
//!
//! - `project.json`：项目元数据（名称、创建时间、最近打开时间）
//! - `AIGC.md`：项目记忆 —— 用户可编辑的项目级 Agent 指令（创作偏好、
//!   品牌规范、常用风格），每次对话时注入 Agent 系统提示词。
//!
//! 另外生成输出仍写入 `<工作目录>/generated/`（见 project_output.rs）。

use std::path::{Path, PathBuf};

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub const FOLDER_NAME: &str = ".openaigc";
pub const META_FILE: &str = "project.json";
pub const MEMORY_FILE: &str = "AIGC.md";

/// 新建项目时写入的 AIGC.md 模板。已有文件永不覆盖。
const MEMORY_TEMPLATE: &str = r#"# 项目说明

这个文件由 OPEN AIGC 创建和读取：每次对话时，这里的内容会作为项目级记忆
注入 Agent，影响它的分析与生成行为。用普通文字写下你的项目约定即可。

建议写：
- 视觉风格与调性（例如：扁平插画，主色 #4F46E5，浅色背景）
- 图片/视频的用途与受众（例如：电商主图，白底产品图）
- 禁止出现的内容（例如：二维码、水印、竞品 logo）
"#;

/// `.openaigc` 文件夹路径。
pub fn folder_path(root: &Path) -> PathBuf {
    root.join(FOLDER_NAME)
}

/// 确保项目文件夹存在，并刷新 `last_opened_at`。
///
/// - 文件夹/AIGC.md 缺失时创建（AIGC.md 只在缺失时写入模板，不覆盖用户编辑）
/// - project.json 缺失或损坏时重建；存在时只更新 last_opened_at
///
/// 尽力而为：任何失败只记日志，不阻塞目录设置主流程。
pub fn ensure_project_folder(root: &Path) {
    let folder = folder_path(root);
    if let Err(e) = std::fs::create_dir_all(&folder) {
        eprintln!(
            "[ProjectFolder] create_dir_all failed: {}: {e}",
            folder.display()
        );
        return;
    }

    let now = now_rfc3339();
    let name = root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string());

    write_memory_if_missing(&folder);
    update_meta(&folder, &name, &now);
}

/// 读取项目记忆（`.openaigc/AIGC.md`）。
///
/// 缺失或读取失败时返回 None（记忆注入是尽力而为的增强，不阻塞对话）。
pub fn read_project_memory(root: &Path) -> Option<String> {
    let path = folder_path(root).join(MEMORY_FILE);
    let content = std::fs::read_to_string(&path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_owned())
}

fn write_memory_if_missing(folder: &Path) {
    let path = folder.join(MEMORY_FILE);
    if path.is_file() {
        return;
    }
    if let Err(e) = std::fs::write(&path, MEMORY_TEMPLATE) {
        eprintln!("[ProjectFolder] write memory template failed: {e}");
    }
}

fn update_meta(folder: &Path, name: &str, now: &str) {
    let path = folder.join(META_FILE);
    let mut meta = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::json!({ "version": 1 }));

    let created = meta
        .get("created_at")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| now.to_owned());
    meta["version"] = serde_json::json!(1);
    meta["name"] = serde_json::json!(name);
    meta["created_at"] = serde_json::json!(created);
    meta["last_opened_at"] = serde_json::json!(now);

    if let Ok(pretty) = serde_json::to_string_pretty(&meta) {
        if let Err(e) = std::fs::write(&path, pretty + "\n") {
            eprintln!("[ProjectFolder] write meta failed: {e}");
        }
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_creates_folder_and_files() {
        let root = std::env::temp_dir().join("aigc-project-folder-ensure");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        ensure_project_folder(&root);

        let meta_path = folder_path(&root).join(META_FILE);
        let memory_path = folder_path(&root).join(MEMORY_FILE);
        assert!(meta_path.is_file());
        assert!(memory_path.is_file());

        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
        assert_eq!(meta["version"], 1);
        assert_eq!(meta["name"], "aigc-project-folder-ensure");
        assert!(meta["created_at"].as_str().is_some());

        let memory = std::fs::read_to_string(&memory_path).unwrap();
        assert!(memory.contains("# 项目说明"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ensure_preserves_created_at_and_user_memory() {
        let root = std::env::temp_dir().join("aigc-project-folder-preserve");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        ensure_project_folder(&root);
        let folder = folder_path(&root);
        let meta_path = folder.join(META_FILE);
        let original: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();

        // 用户编辑了记忆文件
        std::fs::write(folder.join(MEMORY_FILE), "# 项目说明\n自定义风格").unwrap();
        ensure_project_folder(&root);

        let updated: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
        assert_eq!(updated["created_at"], original["created_at"]);
        assert_eq!(
            std::fs::read_to_string(folder.join(MEMORY_FILE)).unwrap(),
            "# 项目说明\n自定义风格",
            "用户编辑的记忆不能被模板覆盖"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn read_memory_returns_content_and_none() {
        let root = std::env::temp_dir().join("aigc-project-folder-read");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        assert!(read_project_memory(&root).is_none());

        std::fs::create_dir_all(folder_path(&root)).unwrap();
        std::fs::write(folder_path(&root).join(MEMORY_FILE), "  自定义记忆  \n").unwrap();
        assert_eq!(read_project_memory(&root).as_deref(), Some("自定义记忆"));

        // 空文件视为无记忆
        std::fs::write(folder_path(&root).join(MEMORY_FILE), "   ").unwrap();
        assert!(read_project_memory(&root).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
