//! 用户指定的工作目录（项目输出目录）导出。
//!
//! 前端选择目录后通过 `agent_v1_set_output_directory` 同步到后端；
//! 生成流水线在受管资产入库后，若已指定工作目录，则额外把结果文件
//! 导出到 `<工作目录>/generated/<对话标题或日期>/`。
//! 未指定工作目录时保持默认：仅写入 workspace 的 downloads / managed-files。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

static OUTPUT_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// 设置（或清除）用户指定的工作目录。
pub fn set_output_dir(path: Option<PathBuf>) {
    if let Ok(mut guard) = OUTPUT_DIR.lock() {
        *guard = path.filter(|p| {
            let s = p.to_string_lossy().trim().to_owned();
            !s.is_empty()
        });
    }
}

/// 读取当前工作目录（未指定时为 None）。
pub fn current_output_dir() -> Option<PathBuf> {
    OUTPUT_DIR
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
        .filter(|p| !p.as_os_str().is_empty())
}

/// 将生成结果导出到当前工作目录。
///
/// - `source`：pipeline 下载到 workspace 的本地文件
/// - `task_id`：生成任务 ID，用于反查对话标题
/// - `database_path`：可选，用于查询对话标题
///
/// 返回导出后的绝对路径；未指定工作目录或导出失败时返回 None（不阻塞主流程）。
pub fn export_generated_file(
    source: &Path,
    task_id: &str,
    database_path: Option<&Path>,
) -> Option<PathBuf> {
    let root = current_output_dir()?;
    if !source.is_file() {
        return None;
    }

    let folder = resolve_export_folder(task_id, database_path);
    let dest_dir = root.join("generated").join(&folder);
    if std::fs::create_dir_all(&dest_dir).is_err() {
        eprintln!(
            "[ProjectOutput] create_dir_all failed: {}",
            dest_dir.display()
        );
        return None;
    }

    let file_name = source.file_name()?;
    let dest = dest_dir.join(file_name);
    if dest.is_file() {
        return Some(dest);
    }

    match std::fs::copy(source, &dest) {
        Ok(_) => {
            eprintln!(
                "[ProjectOutput] exported {} -> {}",
                source.display(),
                dest.display()
            );
            Some(dest)
        }
        Err(e) => {
            eprintln!(
                "[ProjectOutput] copy failed {} -> {}: {e}",
                source.display(),
                dest.display()
            );
            None
        }
    }
}

/// 解析导出子目录：优先对话标题，否则用 UTC 日期，再退化为 task_id 前缀。
fn resolve_export_folder(task_id: &str, database_path: Option<&Path>) -> String {
    if let Some(db) = database_path {
        if let Some(title) = lookup_conversation_title(db, task_id) {
            let sanitized = sanitize_folder_name(&title);
            if !sanitized.is_empty() {
                return sanitized;
            }
        }
    }

    let now = time::OffsetDateTime::now_utc();
    let date = format!(
        "{:04}-{:02}-{:02}",
        now.year(),
        u8::from(now.month()),
        now.day()
    );
    if date.len() == 10 {
        return date;
    }

    let short = task_id.chars().take(8).collect::<String>();
    if short.is_empty() {
        "unknown".to_owned()
    } else {
        short
    }
}

fn lookup_conversation_title(database_path: &Path, task_id: &str) -> Option<String> {
    let conn = rusqlite::Connection::open_with_flags(
        database_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    let mut stmt = conn
        .prepare(
            "SELECT c.title \
             FROM agent_tool_invocations i \
             JOIN agent_conversations c ON c.id = i.conversation_id \
             WHERE i.generation_task_id = ?1 \
             LIMIT 1",
        )
        .ok()?;
    let title: Option<String> = stmt.query_row([task_id], |row| row.get(0)).ok()?;
    title
}

/// Windows 目录名安全化：去掉非法字符，限制长度。
fn sanitize_folder_name(title: &str) -> String {
    let cleaned: String = title
        .trim()
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 32 => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim_matches(['.', ' ']).trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // 限制长度，避免路径过长
    let mut out: String = trimmed.chars().take(40).collect();
    if out.trim().is_empty() {
        out = trimmed.chars().take(8).collect();
    }
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// 全局 OUTPUT_DIR 在测试间共享，串行化避免竞态。
    static TEST_LOCK: StdMutex<()> = StdMutex::new(());

    #[test]
    fn sanitize_replaces_illegal_chars() {
        assert_eq!(sanitize_folder_name("a/b\\c:d"), "a_b_c_d");
        assert!(sanitize_folder_name("   ").is_empty());
    }

    #[test]
    fn export_noop_without_output_dir() {
        let _g = TEST_LOCK.lock().unwrap();
        set_output_dir(None);
        let dir = std::env::temp_dir().join("aigc-project-output-test-none");
        let _ = std::fs::create_dir_all(&dir);
        let src = dir.join("x.png");
        std::fs::write(&src, b"png").unwrap();
        assert!(export_generated_file(&src, "task-1", None).is_none());
        set_output_dir(None);
    }

    #[test]
    fn export_copies_into_generated_subdir() {
        let _g = TEST_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join("aigc-project-output-test-export");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        set_output_dir(Some(root.clone()));

        let src_dir = root.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let src = src_dir.join("jimeng-test.png");
        std::fs::write(&src, b"fake-png").unwrap();

        let dest = export_generated_file(&src, "task-abc", None).expect("should export");
        assert!(dest.starts_with(root.join("generated")));
        assert!(dest.ends_with("jimeng-test.png"));
        assert!(dest.is_file());

        set_output_dir(None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
