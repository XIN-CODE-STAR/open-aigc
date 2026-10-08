//! 诊断日志：把关键失败路径从「只在 stderr 上闪一下」变成「落盘可事后取证」。
//!
//! 为什么需要它
//! ------------
//! 2026-10-08 画布便签故障（执行计划 0%、唯一一步红叉）复盘时发现：
//! 真实异常走的是 `eprintln!`（stderr），而 Tauri CLI 派生进程的 stderr
//! **没有**落到 `.workbuddy-ai/dev-launch.log`。结果是——失败发生了、
//! 数据库里只留下 `status=failed`，**异常文本永久丢失**，整条链路无法追查。
//!
//! 判例：**只往 stderr 写错误，等于在这样一个环境里不写错误。**
//! 诊断信息必须落到一个「宿主是否重定向 stdout/stderr」都不影响的位置。
//!
//! 设计取舍
//! --------
//! - **同步 append**：失败路径本就罕见，一次 open/write/close 的开销可忽略；
//!   换来的是「进程突然中断也不丢最后一行」。
//! - **失败静默**：诊断本身绝不能反过来搞崩业务。任何 IO 错误一律吞掉，
//!   并在 stderr 留一行提示（stderr 有则更好，无则无妨）。
//! - **不引入依赖**：不新增 crate（`log`/`tracing` 需要额外初始化与 filter
//!   配置），用最朴素的 `OpenOptions::append` 即可满足「能取证」这一目标。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// 日志文件名：与数据库同目录（`workspace/`），随应用数据目录走。
const LOG_FILE_NAME: &str = "agent-diagnostics.log";

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// 在应用启动时登记日志路径（workspace 目录）。重复调用只有第一次生效。
pub fn init(workspace_dir: impl AsRef<Path>) {
    let _ = LOG_PATH.set(workspace_dir.as_ref().join(LOG_FILE_NAME));
}

/// 追加一行诊断。未调用过 [`init`] 时退化为 stderr，绝不 panic。
pub fn log(scope: &str, message: &str) {
    let line = format!(
        "{} [{scope}] {message}\n",
        crate::domain::execution::now_rfc3339()
    );
    let Some(path) = LOG_PATH.get() else {
        eprint!("{line}");
        return;
    };
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        Ok(mut file) => {
            if file.write_all(line.as_bytes()).is_err() {
                eprint!("{line}");
            }
        }
        Err(e) => {
            eprintln!("[diagnostics] cannot open log file {}: {e}", path.display());
            eprint!("{line}");
        }
    }
}

/// 记录一次失败（`scope` 用模块/函数名，便于 grep）。
#[macro_export]
macro_rules! diag_fail {
    ($scope:expr, $($arg:tt)*) => {
        $crate::application::diagnostics::log($scope, &format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 初始化后确实落盘，且是追加语义（两行都在）。
    ///
    /// 注意：`LOG_PATH` 是进程级 `OnceLock`，同进程内先设者生效。
    /// 因此本用例不断言「文件里恰好只有这两行」，只断言可观测性质：
    /// 路径已登记 + 写入后文件存在且包含写过的内容。
    #[test]
    fn log_appends_to_file() {
        let dir = std::env::temp_dir().join(format!("aigc-diag-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        init(&dir);
        let path = LOG_PATH
            .get()
            .expect("init 后 LOG_PATH 必须已登记")
            .clone();
        log("test", "first line");
        log("test", "second line");
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            content.contains("first line"),
            "写入后文件应包含日志内容，实际路径 {:?} 内容: {content:?}",
            path
        );
        assert!(
            content.contains("second line"),
            "追加语义：第二行也必须在，实际: {content:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
