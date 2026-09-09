#![allow(dead_code)]
//! Sandbox Policy：工具执行沙箱策略。
//!
//! 定义工具执行时的权限边界：文件路径限制、网络策略、资源上限。
//! 设计目标：在不引入 Docker/容器的前提下，通过声明式策略约束 Agent 工具执行。
//!
//! 当前为定义 + 校验阶段。实际拦截逻辑在后续 Phase 接入 ToolContext 时实现。

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

// ─── NetworkPolicy ───

/// 网络访问策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkPolicy {
    /// 允许所有网络访问。
    AllowAll,
    /// 只允许已知安全域名（Provider API、下载 CDN）。
    AllowKnown,
    /// 禁止所有网络访问。
    DenyAll,
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self::AllowKnown
    }
}

// ─── SandboxPolicy ───

/// 工具执行沙箱策略。
///
/// 声明式定义工具执行的权限边界。
/// AgentToolExecutor 在执行工具前应检查此策略。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxPolicy {
    /// 允许访问的路径前缀列表。
    /// 工具只能读写匹配这些前缀的路径。
    /// 默认: [workspace/**]
    pub allowed_paths: Vec<PathBuf>,

    /// 禁止访问的路径前缀列表（优先级高于 allowed_paths）。
    /// 默认: [C:\Windows, ~/.ssh, ~/.gnupg, ...]
    pub denied_paths: Vec<PathBuf>,

    /// 网络访问策略。
    /// 默认: AllowKnown（只允许 Provider API 等已知域名）
    pub network: NetworkPolicy,

    /// 单次工具执行最大运行时间。
    /// 默认: 300 秒
    pub max_runtime: Duration,

    /// 工具输出最大字节数。
    /// 默认: 100 MB
    pub max_output_bytes: u64,

    /// 是否允许执行系统命令（shell、ffmpeg 等）。
    /// 默认: false
    pub allow_process_spawn: bool,

    /// 允许 spawn 的命令白名单（allow_process_spawn 为 true 时生效）。
    /// 默认: ["ffmpeg", "ffprobe"]
    pub allowed_commands: Vec<String>,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            allowed_paths: Vec::new(), // 运行时注入 workspace_path
            denied_paths: default_denied_paths(),
            network: NetworkPolicy::AllowKnown,
            max_runtime: Duration::from_secs(300),
            max_output_bytes: 100 * 1024 * 1024, // 100 MB
            allow_process_spawn: false,
            allowed_commands: vec!["ffmpeg".to_owned(), "ffprobe".to_owned()],
        }
    }
}

impl SandboxPolicy {
    /// 创建宽松策略（开发/测试用）。
    pub fn permissive() -> Self {
        Self {
            allowed_paths: Vec::new(),
            denied_paths: Vec::new(),
            network: NetworkPolicy::AllowAll,
            max_runtime: Duration::from_secs(600),
            max_output_bytes: 500 * 1024 * 1024, // 500 MB
            allow_process_spawn: true,
            allowed_commands: vec![
                "ffmpeg".to_owned(),
                "ffprobe".to_owned(),
                "git".to_owned(),
                "npm".to_owned(),
                "node".to_owned(),
            ],
        }
    }

    /// 创建严格策略（最小权限）。
    pub fn strict() -> Self {
        Self {
            allowed_paths: Vec::new(),
            denied_paths: default_denied_paths(),
            network: NetworkPolicy::DenyAll,
            max_runtime: Duration::from_secs(60),
            max_output_bytes: 10 * 1024 * 1024, // 10 MB
            allow_process_spawn: false,
            allowed_commands: Vec::new(),
        }
    }

    /// 设置允许的 workspace 路径。
    pub fn with_workspace(mut self, workspace_path: PathBuf) -> Self {
        self.allowed_paths.push(workspace_path);
        self
    }

    /// 检查路径是否在沙箱允许范围内。
    pub fn is_path_allowed(&self, path: &std::path::Path) -> bool {
        // 黑名单优先
        for denied in &self.denied_paths {
            if path.starts_with(denied) {
                return false;
            }
        }

        // 如果没有允许列表，默认全部允许（黑名单除外）
        if self.allowed_paths.is_empty() {
            return true;
        }

        // 白名单匹配
        self.allowed_paths
            .iter()
            .any(|allowed| path.starts_with(allowed))
    }

    /// 检查命令是否允许执行。
    pub fn is_command_allowed(&self, command: &str) -> bool {
        if !self.allow_process_spawn {
            return false;
        }

        // 提取命令基名（去掉路径前缀）
        let base = std::path::Path::new(command)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(command);

        self.allowed_commands.iter().any(|allowed| allowed == base)
    }
}

/// 默认禁止路径列表。
fn default_denied_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Windows 系统目录
    #[cfg(target_os = "windows")]
    {
        if let Ok(windir) = std::env::var("WINDIR") {
            paths.push(PathBuf::from(&windir));
        }
        paths.push(PathBuf::from("C:\\Windows"));
        if let Ok(pf) = std::env::var("ProgramFiles") {
            paths.push(PathBuf::from(pf));
        }
    }

    // Unix 敏感目录
    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(home) = std::env::var("HOME") {
            let home_path = PathBuf::from(&home);
            paths.push(home_path.join(".ssh"));
            paths.push(home_path.join(".gnupg"));
            paths.push(home_path.join(".aws"));
            paths.push(home_path.join(".kube"));
        }
        paths.push(PathBuf::from("/etc"));
        paths.push(PathBuf::from("/sys"));
        paths.push(PathBuf::from("/proc"));
    }

    // 跨平台敏感目录
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        let home_path = PathBuf::from(&home);
        paths.push(home_path.join(".ssh"));
        paths.push(home_path.join(".gnupg"));
        paths.push(home_path.join(".aws"));
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_default_policy() {
        let policy = SandboxPolicy::default();
        assert_eq!(policy.network, NetworkPolicy::AllowKnown);
        assert!(!policy.allow_process_spawn);
        assert_eq!(policy.max_runtime, Duration::from_secs(300));
    }

    #[test]
    fn test_path_allowed_no_whitelist() {
        let policy = SandboxPolicy {
            allowed_paths: vec![],
            ..Default::default()
        };
        // 无白名单时，除黑名单外都允许
        assert!(policy.is_path_allowed(Path::new("/tmp/test.png")));
    }

    #[test]
    fn test_path_allowed_with_workspace() {
        let policy = SandboxPolicy {
            allowed_paths: vec![PathBuf::from("/workspace")],
            denied_paths: vec![PathBuf::from("/workspace/.ssh")],
            ..Default::default()
        };
        assert!(policy.is_path_allowed(Path::new("/workspace/assets/test.png")));
        assert!(!policy.is_path_allowed(Path::new("/other/path")));
        assert!(!policy.is_path_allowed(Path::new("/workspace/.ssh/id_rsa")));
    }

    #[test]
    fn test_command_allowed() {
        let policy = SandboxPolicy {
            allow_process_spawn: true,
            allowed_commands: vec!["ffmpeg".to_owned(), "ffprobe".to_owned()],
            ..Default::default()
        };
        assert!(policy.is_command_allowed("ffmpeg"));
        assert!(policy.is_command_allowed("/usr/bin/ffmpeg"));
        assert!(!policy.is_command_allowed("rm"));
        assert!(!policy.is_command_allowed("bash"));
    }

    #[test]
    fn test_command_not_allowed_when_spawn_disabled() {
        let policy = SandboxPolicy {
            allow_process_spawn: false,
            ..Default::default()
        };
        assert!(!policy.is_command_allowed("ffmpeg"));
    }

    #[test]
    fn test_permissive_policy() {
        let policy = SandboxPolicy::permissive();
        assert_eq!(policy.network, NetworkPolicy::AllowAll);
        assert!(policy.allow_process_spawn);
    }

    #[test]
    fn test_strict_policy() {
        let policy = SandboxPolicy::strict();
        assert_eq!(policy.network, NetworkPolicy::DenyAll);
        assert!(!policy.allow_process_spawn);
    }

    #[test]
    fn test_policy_serde() {
        let policy = SandboxPolicy::default();
        let json = serde_json::to_string(&policy).unwrap();
        let parsed: SandboxPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.network, NetworkPolicy::AllowKnown);
        assert_eq!(parsed.max_runtime, Duration::from_secs(300));
    }
}
