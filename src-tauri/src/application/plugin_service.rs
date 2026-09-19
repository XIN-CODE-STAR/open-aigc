//! 插件中心：用户技能（SKILL.md 目录）与 MCP 服务器的发现 / 导入 / 下载 / 管理。
//!
//! 存储布局（工作区目录 = aigc-studio.sqlite3 所在目录）：
//! - `<workspace>/skills/<slug>/SKILL.md` —— 技能；SKILL.md 支持可选 YAML 风格
//!   frontmatter（name / description 两个键），缺省回退目录名与首段文本。
//! - `<workspace>/mcp_servers.json` —— MCP stdio 服务器配置数组。
//!
//! 下载来源支持：raw SKILL.md / 单文件 .md、zip 归档（根目录或一层子目录内
//! 含 SKILL.md）、GitHub 仓库地址（自动转 codeload zip）。

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::application::error::AppError;
use crate::ports::persistence::PersistenceError;

pub const SKILLS_DIR_NAME: &str = "skills";
pub const MCP_CONFIG_FILE: &str = "mcp_servers.json";
const SKILL_ENTRY_FILE: &str = "SKILL.md";
const DOWNLOAD_MAX_BYTES: usize = 50 * 1024 * 1024;
const SKILL_WALK_DEPTH: usize = 3;

pub fn skills_dir(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join(SKILLS_DIR_NAME)
}

pub fn mcp_config_path(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join(MCP_CONFIG_FILE)
}

fn io_err(operation: &'static str, error: std::io::Error) -> AppError {
    AppError::Persistence(PersistenceError::new(operation, error))
}

// ──────────────────────────────────────────────────────────────────
// 技能（SKILL.md 目录）
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMeta {
    pub slug: String,
    pub name: String,
    pub description: String,
    /// local-import / download / manual
    pub source: String,
    pub installed_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCandidate {
    pub name: String,
    pub description: String,
    /// 含 SKILL.md 的目录绝对路径
    pub path: String,
}

struct ParsedSkill {
    name: String,
    description: String,
}

/// 解析 SKILL.md：支持 `---` 包裹的 frontmatter（name / description），缺省回退
/// 首个非空行为名称、次行为描述。
fn parse_skill_md(content: &str) -> ParsedSkill {
    let trimmed = content.trim_start();
    if let Some(rest) = trimmed.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            let frontmatter = &rest[..end];
            let mut name = None;
            let mut description = None;
            for line in frontmatter.lines() {
                let Some((key, value)) = line.split_once(':') else {
                    continue;
                };
                let value = value.trim().trim_matches('"').trim_matches('\'').to_owned();
                match key.trim().to_lowercase().as_str() {
                    "name" if !value.is_empty() => name = Some(value),
                    "description" if !value.is_empty() => description = Some(value),
                    _ => {}
                }
            }
            return ParsedSkill {
                name: name.unwrap_or_else(String::new),
                description: description.unwrap_or_default(),
            };
        }
    }
    // 无 frontmatter：首个非空行为名称，次非空行为描述
    let mut lines = content.lines().map(str::trim).filter(|l| !l.is_empty());
    ParsedSkill {
        name: lines.next().unwrap_or_default().to_owned(),
        description: lines.next().unwrap_or_default().to_owned(),
    }
}

fn read_skill_entry(dir: &Path) -> Option<ParsedSkill> {
    let content = fs::read_to_string(dir.join(SKILL_ENTRY_FILE)).ok()?;
    Some(parse_skill_md(&content))
}

fn skill_meta_from_dir(
    workspace_dir: &Path,
    dir: &Path,
    fallback_source: &str,
) -> Option<SkillMeta> {
    let slug = dir.file_name()?.to_str()?.to_owned();
    let parsed = read_skill_entry(dir)?;
    let installed_at = fs::metadata(dir.join(SKILL_ENTRY_FILE))
        .and_then(|m| m.modified())
        .ok()
        .map(|t| {
            let seconds = t
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            format!("{seconds}")
        })
        .unwrap_or_default();
    let _ = workspace_dir;
    Some(SkillMeta {
        slug: slug.clone(),
        name: if parsed.name.is_empty() {
            slug
        } else {
            parsed.name
        },
        description: parsed.description,
        source: fallback_source.to_owned(),
        installed_at,
    })
}

/// 列出工作区已安装技能（目录含 SKILL.md 即视为技能）。
pub fn list_skills(workspace_dir: &Path) -> Result<Vec<SkillMeta>, AppError> {
    let dir = skills_dir(workspace_dir);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut skills = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| io_err("list skills", e))? {
        let entry = entry.map_err(|e| io_err("read skill entry", e))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(meta) = skill_meta_from_dir(workspace_dir, &path, "manual") {
            skills.push(meta);
        }
    }
    skills.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(skills)
}

/// 读取技能正文（frontmatter 之后的内容），供 Agent 的 use_skill 工具使用。
pub fn get_skill_body(workspace_dir: &Path, slug: &str) -> Result<String, AppError> {
    let path = skills_dir(workspace_dir).join(slug).join(SKILL_ENTRY_FILE);
    let content = fs::read_to_string(&path).map_err(|e| io_err("read skill body", e))?;
    let body = match content.trim_start().strip_prefix("---") {
        Some(rest) => match rest.find("\n---") {
            Some(end) => rest[end + 4..].trim_start().to_owned(),
            None => content,
        },
        None => content,
    };
    Ok(body)
}

pub fn delete_skill(workspace_dir: &Path, slug: &str) -> Result<(), AppError> {
    let dir = skills_dir(workspace_dir).join(slug);
    // 防御：slug 不得逃出 skills 目录
    if slug.contains("..") || slug.contains('/') || slug.contains('\\') || slug.is_empty() {
        return Err(AppError::Persistence(PersistenceError::new(
            "delete skill",
            std::io::Error::other("invalid skill slug"),
        )));
    }
    if !dir.exists() {
        return Ok(());
    }
    fs::remove_dir_all(&dir).map_err(|e| io_err("delete skill", e))
}

/// 在本地目录（最多向下 SKILL_WALK_DEPTH 层）扫描包含 SKILL.md 的技能目录。
pub fn scan_local_skills(scan_root: &str) -> Result<Vec<SkillCandidate>, AppError> {
    let root = PathBuf::from(scan_root);
    if !root.exists() {
        return Err(io_err(
            "scan local skills",
            std::io::Error::new(std::io::ErrorKind::NotFound, "目录不存在"),
        ));
    }
    let mut found = Vec::new();
    walk_skill_dirs(&root, 0, &mut found);
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

fn walk_skill_dirs(dir: &Path, depth: usize, found: &mut Vec<SkillCandidate>) {
    if depth > SKILL_WALK_DEPTH || found.len() >= 100 {
        return;
    }
    if dir.join(SKILL_ENTRY_FILE).is_file() {
        if let Some(parsed) = read_skill_entry(dir) {
            let path_text = dir.to_string_lossy().to_string();
            found.push(SkillCandidate {
                name: if parsed.name.is_empty() {
                    dir.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path_text.clone())
                } else {
                    parsed.name
                },
                description: parsed.description,
                path: path_text,
            });
        }
        return; // 技能目录不再向下递归
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_skill_dirs(&path, depth + 1, found);
        }
    }
}

/// 从本地路径导入技能：支持技能目录或单个 .md 文件。
pub fn import_skill_from_path(
    workspace_dir: &Path,
    source_path: &str,
    source_tag: &str,
) -> Result<SkillMeta, AppError> {
    let source = PathBuf::from(source_path);
    if !source.exists() {
        return Err(io_err(
            "import skill",
            std::io::Error::new(std::io::ErrorKind::NotFound, "源路径不存在"),
        ));
    }
    if source.is_file() {
        install_skill_file(workspace_dir, &source, source_tag)
    } else {
        install_skill_dir(workspace_dir, &source, source_tag)
    }
}

fn next_available_dir(base: &Path, slug: &str) -> PathBuf {
    let mut candidate = base.join(slug);
    let mut suffix = 2;
    while candidate.exists() {
        candidate = base.join(format!("{slug}-{suffix}"));
        suffix += 1;
    }
    candidate
}

/// 安装一个技能目录（拷贝目录树，可包含 SKILL.md 同级资源文件）。
fn install_skill_dir(
    workspace_dir: &Path,
    source_dir: &Path,
    source_tag: &str,
) -> Result<SkillMeta, AppError> {
    if !source_dir.join(SKILL_ENTRY_FILE).is_file() {
        return Err(io_err(
            "install skill",
            std::io::Error::new(std::io::ErrorKind::InvalidData, "目录缺少 SKILL.md"),
        ));
    }
    let base = skills_dir(workspace_dir);
    fs::create_dir_all(&base).map_err(|e| io_err("create skills dir", e))?;
    let raw_name = source_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "skill".to_owned());
    let slug = next_available_dir(&base, &slugify(&raw_name));
    copy_dir_recursive(source_dir, &slug)?;
    let meta = skill_meta_from_dir(workspace_dir, &slug, source_tag)
        .ok_or_else(|| io_err("install skill", std::io::Error::other("安装后无法读取技能")))?;
    Ok(meta)
}

/// 安装单个 .md 文件为技能（<workspace>/skills/<stem>/SKILL.md）。
fn install_skill_file(
    workspace_dir: &Path,
    source_file: &Path,
    source_tag: &str,
) -> Result<SkillMeta, AppError> {
    let base = skills_dir(workspace_dir);
    fs::create_dir_all(&base).map_err(|e| io_err("create skills dir", e))?;
    let stem = source_file
        .file_stem()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "skill".to_owned());
    let dir = next_available_dir(&base, &slugify(&stem));
    fs::create_dir_all(&dir).map_err(|e| io_err("create skill dir", e))?;
    fs::copy(source_file, dir.join(SKILL_ENTRY_FILE)).map_err(|e| io_err("copy skill file", e))?;
    skill_meta_from_dir(workspace_dir, &dir, source_tag)
        .ok_or_else(|| io_err("install skill", std::io::Error::other("安装后无法读取技能")))
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), AppError> {
    // 防自拷贝：source 位于 target 内会导致无限递归
    if let (Ok(src), Ok(dst)) = (source.canonicalize(), target.canonicalize()) {
        if src == dst || src.starts_with(&dst) {
            return Err(io_err(
                "install skill",
                std::io::Error::other("源目录位于目标目录内"),
            ));
        }
    }
    fs::create_dir_all(target).map_err(|e| io_err("create skill dir", e))?;
    for entry in fs::read_dir(source).map_err(|e| io_err("read skill source", e))? {
        let entry = entry.map_err(|e| io_err("read skill source entry", e))?;
        let path = entry.path();
        let dest = target.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &dest)?;
        } else if path.is_file() {
            fs::copy(&path, &dest).map_err(|e| io_err("copy skill file", e))?;
        }
    }
    Ok(())
}

/// slug 化：小写、空白与非 [a-z0-9-_] 替换为 '-'，压缩连续 '-'，限 48 字符。
fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in name.trim().chars() {
        let mapped = if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            ch.to_ascii_lowercase()
        } else if ch.is_alphanumeric() {
            ch // 保留中文等 Unicode 字母
        } else {
            '-'
        };
        if mapped == '-' && last_dash {
            continue;
        }
        last_dash = mapped == '-';
        out.push(mapped);
    }
    let trimmed = out.trim_matches('-').to_owned();
    let trimmed = if trimmed.is_empty() {
        "skill".to_owned()
    } else {
        trimmed
    };
    let mut result: String = trimmed.chars().take(48).collect();
    if result.is_empty() {
        result = "skill".to_owned();
    }
    result
}

// ──────────────────────────────────────────────────────────────────
// 下载安装
// ──────────────────────────────────────────────────────────────────

fn fetch_url(url: &str) -> Result<Vec<u8>, AppError> {
    let response = ureq::get(url)
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(|e| io_err("download skill", std::io::Error::other(e.to_string())))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(DOWNLOAD_MAX_BYTES as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| io_err("read download", e))?;
    Ok(bytes)
}

/// GitHub 仓库页 → codeload zip（先 main 后 master）。
fn github_zip_urls(url: &str) -> Vec<String> {
    let trimmed = url.trim_end_matches('/');
    let Some(rest) = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
    else {
        return Vec::new();
    };
    // 去掉可能的 /tree/<branch> 后缀
    let repo = rest
        .split("/tree/")
        .next()
        .unwrap_or(rest)
        .trim_end_matches('/');
    let parts: Vec<&str> = repo.split('/').collect();
    if parts.len() < 2 {
        return Vec::new();
    }
    vec![
        format!(
            "https://codeload.github.com/{}/{}/zip/refs/heads/main",
            parts[0], parts[1]
        ),
        format!(
            "https://codeload.github.com/{}/{}/zip/refs/heads/master",
            parts[0], parts[1]
        ),
    ]
}

fn is_zip_bytes(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK")
}

/// 生产代码用的临时目录（uuid 命名，调用方负责 best-effort 清理）。
fn make_temp_dir(label: &str) -> Result<PathBuf, AppError> {
    let dir = std::env::temp_dir().join(format!("aigc-{label}-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).map_err(|e| io_err("create temp dir", e))?;
    Ok(dir)
}

fn cleanup_temp(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

/// 解压 zip 到临时目录，返回解压根。
fn unzip_to_temp(bytes: &[u8]) -> Result<(PathBuf, PathBuf), AppError> {
    let reader = std::io::Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| io_err("open zip", std::io::Error::other(e)))?;
    let temp = make_temp_dir("skill-zip")?;
    let mut root = temp.clone();
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| io_err("read zip entry", std::io::Error::other(e)))?;
        let Some(relative) = file.enclosed_name() else {
            continue; // 跳过路径穿越条目
        };
        let dest = temp.join(relative);
        if file.is_dir() {
            fs::create_dir_all(&dest).map_err(|e| io_err("unzip dir", e))?;
            continue;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| io_err("unzip parent", e))?;
        }
        let mut out = fs::File::create(&dest).map_err(|e| io_err("unzip file", e))?;
        std::io::copy(&mut file, &mut out).map_err(|e| io_err("unzip copy", e))?;
    }
    // 单根目录包裹（GitHub zip 常见）：进入该目录
    if !root.join(SKILL_ENTRY_FILE).exists() {
        if let Ok(entries) = fs::read_dir(&root) {
            let dirs: Vec<_> = entries.flatten().filter(|e| e.path().is_dir()).collect();
            if dirs.len() == 1 {
                root = dirs[0].path();
            }
        }
    }
    Ok((temp, root))
}

/// 从 zip 归档内容中安装技能：定位含 SKILL.md 的目录（根目录或一层子目录）。
fn install_skill_from_extracted(
    workspace_dir: &Path,
    extract_root: &Path,
    source_tag: &str,
) -> Result<SkillMeta, AppError> {
    if extract_root.join(SKILL_ENTRY_FILE).is_file() {
        return install_skill_dir(workspace_dir, extract_root, source_tag);
    }
    let Ok(entries) = fs::read_dir(extract_root) else {
        return Err(io_err(
            "install skill",
            std::io::Error::new(std::io::ErrorKind::InvalidData, "压缩包为空"),
        ));
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.join(SKILL_ENTRY_FILE).is_file() {
            return install_skill_dir(workspace_dir, &path, source_tag);
        }
    }
    Err(io_err(
        "install skill",
        std::io::Error::new(std::io::ErrorKind::InvalidData, "压缩包内未找到 SKILL.md"),
    ))
}

/// 下载并安装技能：
/// - `.md` 直链 → 单文件技能；
/// - zip 归档 → 解压找 SKILL.md；
/// - GitHub 仓库页 → 自动转 codeload zip（main → master）。
pub fn download_skill(workspace_dir: &Path, url: &str) -> Result<SkillMeta, AppError> {
    let trimmed = url.trim();
    let lower = trimmed.to_lowercase();
    if lower.starts_with("https://github.com/") || lower.starts_with("http://github.com/") {
        for candidate in github_zip_urls(trimmed) {
            if let Ok(bytes) = fetch_url(&candidate) {
                if is_zip_bytes(&bytes) {
                    let (temp, root) = unzip_to_temp(&bytes)?;
                    let result = install_skill_from_extracted(workspace_dir, &root, "download");
                    cleanup_temp(&temp);
                    return result;
                }
            }
        }
        return Err(io_err(
            "download skill",
            std::io::Error::other("GitHub 仓库下载失败（检查网络或仓库是否存在）"),
        ));
    }
    let bytes = fetch_url(trimmed)?;
    if lower.ends_with(".md") || (!is_zip_bytes(&bytes) && lower.contains(".md")) {
        let temp = make_temp_dir("skill-md")?;
        let file = temp.join("SKILL.md");
        let result = match fs::write(&file, &bytes) {
            Ok(()) => install_skill_file(workspace_dir, &file, "download"),
            Err(e) => Err(io_err("write temp skill", e)),
        };
        cleanup_temp(&temp);
        return result;
    }
    if is_zip_bytes(&bytes) {
        let (temp, root) = unzip_to_temp(&bytes)?;
        let result = install_skill_from_extracted(workspace_dir, &root, "download");
        cleanup_temp(&temp);
        return result;
    }
    Err(io_err(
        "download skill",
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "不支持的下载内容（仅支持 .md 或 zip）",
        ),
    ))
}

// ──────────────────────────────────────────────────────────────────
// MCP 服务器配置
// ──────────────────────────────────────────────────────────────────

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    #[serde(default = "new_mcp_id")]
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn new_mcp_id() -> String {
    Uuid::new_v4().to_string()
}

fn read_mcp_servers(workspace_dir: &Path) -> Vec<McpServerConfig> {
    let Ok(content) = fs::read_to_string(mcp_config_path(workspace_dir)) else {
        return Vec::new();
    };
    serde_json::from_str(&content).unwrap_or_default()
}

fn write_mcp_servers(workspace_dir: &Path, servers: &[McpServerConfig]) -> Result<(), AppError> {
    let path = mcp_config_path(workspace_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err("create config dir", e))?;
    }
    let content = serde_json::to_string_pretty(servers)
        .map_err(|e| io_err("serialize mcp config", std::io::Error::other(e)))?;
    fs::write(&path, content).map_err(|e| io_err("write mcp config", e))
}

pub fn list_mcp_servers(workspace_dir: &Path) -> Result<Vec<McpServerConfig>, AppError> {
    Ok(read_mcp_servers(workspace_dir))
}

#[allow(clippy::too_many_arguments)]
pub fn add_mcp_server(
    workspace_dir: &Path,
    name: &str,
    command: &str,
    args: &[String],
    env: &std::collections::BTreeMap<String, String>,
    enabled: bool,
) -> Result<McpServerConfig, AppError> {
    let mut servers = read_mcp_servers(workspace_dir);
    if servers.iter().any(|s| s.name == name) {
        return Err(io_err(
            "add mcp server",
            std::io::Error::new(std::io::ErrorKind::AlreadyExists, "同名服务器已存在"),
        ));
    }
    let config = McpServerConfig {
        id: new_mcp_id(),
        name: name.to_owned(),
        command: command.to_owned(),
        args: args.to_vec(),
        env: env.clone(),
        enabled,
    };
    servers.push(config.clone());
    write_mcp_servers(workspace_dir, &servers)?;
    Ok(config)
}

pub fn update_mcp_server(workspace_dir: &Path, config: &McpServerConfig) -> Result<(), AppError> {
    let mut servers = read_mcp_servers(workspace_dir);
    let Some(slot) = servers.iter_mut().find(|s| s.id == config.id) else {
        return Err(io_err(
            "update mcp server",
            std::io::Error::new(std::io::ErrorKind::NotFound, "服务器不存在"),
        ));
    };
    *slot = config.clone();
    write_mcp_servers(workspace_dir, &servers)
}

pub fn remove_mcp_server(workspace_dir: &Path, id: &str) -> Result<(), AppError> {
    let mut servers = read_mcp_servers(workspace_dir);
    servers.retain(|s| s.id != id);
    write_mcp_servers(workspace_dir, &servers)
}

pub fn set_mcp_server_enabled(
    workspace_dir: &Path,
    id: &str,
    enabled: bool,
) -> Result<(), AppError> {
    let mut servers = read_mcp_servers(workspace_dir);
    let Some(slot) = servers.iter_mut().find(|s| s.id == id) else {
        return Err(io_err(
            "toggle mcp server",
            std::io::Error::new(std::io::ErrorKind::NotFound, "服务器不存在"),
        ));
    };
    slot.enabled = enabled;
    write_mcp_servers(workspace_dir, &servers)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpScanCandidate {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: std::collections::BTreeMap<String, String>,
    /// 配置文件来源路径
    pub source_path: String,
}

/// 扫描本机主流 MCP 配置文件（Claude / Cursor / VS Code 等），提取 stdio 服务器候选。
pub fn scan_local_mcp_configs() -> Vec<McpScanCandidate> {
    let mut candidates = Vec::new();
    let Some(home) = home_dir() else {
        return candidates;
    };
    let appdata = std::env::var("APPDATA").map(PathBuf::from).ok();
    let mut config_files = vec![
        home.join(".claude.json"),
        home.join(".cursor").join("mcp.json"),
        home.join(".vscode").join("mcp.json"),
        home.join(".codeium")
            .join("windsurf")
            .join("mcp_config.json"),
    ];
    if let Some(appdata) = appdata {
        config_files.push(appdata.join("Claude").join("claude_desktop_config.json"));
    }
    for file in config_files {
        let Ok(content) = fs::read_to_string(&file) else {
            continue;
        };
        collect_mcp_candidates(&file, &content, &mut candidates);
    }
    candidates
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .ok()
}

/// 从配置 JSON 中提取 `mcpServers`（或 VS Code 的 `servers`）对象里的 stdio 服务器。
fn collect_mcp_candidates(file: &Path, content: &str, candidates: &mut Vec<McpScanCandidate>) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
        return;
    };
    let servers = value
        .get("mcpServers")
        .or_else(|| value.get("servers"))
        .and_then(|v| v.as_object());
    let Some(servers) = servers else {
        return;
    };
    let source_path = file.to_string_lossy().to_string();
    for (name, entry) in servers {
        let command = entry.get("command").and_then(|v| v.as_str());
        let Some(command) = command else {
            continue; // 跳过 http/sse 类型（v1 仅支持 stdio）
        };
        let args = entry
            .get("args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let env = entry
            .get("env")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                    .collect()
            })
            .unwrap_or_default();
        if candidates
            .iter()
            .any(|c| c.name == *name && c.command == command)
        {
            continue;
        }
        candidates.push(McpScanCandidate {
            name: name.clone(),
            command: command.to_owned(),
            args,
            env,
            source_path: source_path.clone(),
        });
    }
}

/// 注入上下文的字符上限（技能/MCP 很多时防止撑爆规划提示词）。
const PLUGIN_CONTEXT_MAX_CHARS: usize = 3000;

/// 给 Agent 规划阶段注入的插件上下文（技能列表 + MCP 服务器概览）。
/// 超出上限时按行截断并提示用 list_skills / mcp_list_tools 查看完整清单。
pub fn load_plugin_context(workspace_dir: &Path) -> Option<String> {
    let mut sections = Vec::new();
    let Ok(skills) = list_skills(workspace_dir) else {
        return None;
    };
    if !skills.is_empty() {
        let lines = skills
            .iter()
            .map(|s| format!("- {}：{}", s.name, s.description))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!(
            "[可用技能]（调用 use_skill 工具加载技能指令后再执行对应任务）\n{lines}"
        ));
    }
    let servers = read_mcp_servers(workspace_dir);
    let enabled: Vec<_> = servers.iter().filter(|s| s.enabled).collect();
    if !enabled.is_empty() {
        let lines = enabled
            .iter()
            .map(|s| format!("- {}（{}）", s.name, s.command))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!(
            "[MCP 服务器]（用 mcp_list_tools 查看某服务器的工具，用 mcp_call 调用）\n{lines}"
        ));
    }
    if sections.is_empty() {
        return None;
    }
    let merged = sections.join("\n\n");
    if merged.chars().count() <= PLUGIN_CONTEXT_MAX_CHARS {
        return Some(merged);
    }
    // 超限：按整行保留前缀，避免把技能描述截成半句
    let mut kept = String::new();
    for line in merged.lines() {
        if kept.chars().count() + line.chars().count() + 1 > PLUGIN_CONTEXT_MAX_CHARS {
            break;
        }
        kept.push_str(line);
        kept.push('\n');
    }
    Some(format!(
        "{kept}\n（内容过多已截断：用 list_skills / mcp_list_tools 查看完整清单）"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill(dir: &Path, name: &str, description: &str) {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join(SKILL_ENTRY_FILE),
            format!("---\nname: {name}\ndescription: {description}\n---\n# {name}\n步骤一"),
        )
        .unwrap();
    }

    #[test]
    fn parse_skill_md_supports_frontmatter_and_fallback() {
        let with_fm = parse_skill_md("---\nname: 去水印\ndescription: \"去掉二维码\"\n---\n正文");
        assert_eq!(with_fm.name, "去水印");
        assert_eq!(with_fm.description, "去掉二维码");

        let plain = parse_skill_md("第一行是名称\n第二行是描述\n更多");
        assert_eq!(plain.name, "第一行是名称");
        assert_eq!(plain.description, "第二行是描述");
    }

    #[test]
    fn skill_lifecycle_import_list_delete() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        let source = dir.path().join("source-skill");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join(SKILL_ENTRY_FILE),
            "---\nname: 测试技能\ndescription: 一个测试\n---\n正文",
        )
        .unwrap();

        let meta =
            import_skill_from_path(workspace, source.to_str().unwrap(), "local-import").unwrap();
        assert_eq!(meta.name, "测试技能");

        let skills = list_skills(workspace).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].slug, meta.slug);

        let body = get_skill_body(workspace, &meta.slug).unwrap();
        assert!(body.contains("正文"));
        assert!(!body.contains("description"));

        delete_skill(workspace, &meta.slug).unwrap();
        assert!(list_skills(workspace).unwrap().is_empty());
    }

    #[test]
    fn scan_local_skills_finds_nested_skill_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_skill(&root.join("collection").join("alpha"), "alpha", "A");
        write_skill(&root.join("collection").join("beta"), "beta", "B");
        fs::write(root.join("collection").join("readme.txt"), "x").unwrap();

        let found = scan_local_skills(root.to_str().unwrap()).unwrap();
        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|c| c.name == "alpha"));
        assert!(found.iter().all(|c| !c.path.contains("SKILL.md")));
    }

    #[test]
    fn slugify_normalizes_names() {
        assert_eq!(slugify("Hello World!"), "hello-world");
        assert_eq!(slugify("  中文 技能  "), "中文-技能");
        assert_eq!(slugify("---"), "skill");
    }

    #[test]
    fn mcp_config_roundtrip_and_uniqueness() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();

        let first = add_mcp_server(
            workspace,
            "fetch",
            "npx",
            &["-y".into(), "mcp-fetch".into()],
            &std::collections::BTreeMap::new(),
            true,
        )
        .unwrap();
        assert!(!first.id.is_empty());

        let mut env = std::collections::BTreeMap::new();
        env.insert("API_KEY".to_owned(), "x".to_owned());
        let second =
            add_mcp_server(workspace, "fs", "node", &["server.js".into()], &env, false).unwrap();

        let all = list_mcp_servers(workspace).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].env.get("API_KEY").map(String::as_str), Some("x"));

        // 同名拒绝
        assert!(add_mcp_server(
            workspace,
            "fetch",
            "npx",
            &[],
            &std::collections::BTreeMap::new(),
            true
        )
        .is_err());

        set_mcp_server_enabled(workspace, &second.id, true).unwrap();
        assert!(list_mcp_servers(workspace).unwrap()[1].enabled);

        update_mcp_server(
            workspace,
            &McpServerConfig {
                args: vec!["-v".to_owned()],
                ..all[1].clone()
            },
        )
        .unwrap();
        assert_eq!(
            list_mcp_servers(workspace).unwrap()[1].args,
            vec!["-v".to_owned()]
        );

        remove_mcp_server(workspace, &first.id).unwrap();
        assert_eq!(list_mcp_servers(workspace).unwrap().len(), 1);
    }

    #[test]
    fn collect_mcp_candidates_parses_known_shapes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("claude.json");
        fs::write(
            &file,
            r#"{"mcpServers":{"fetch":{"command":"npx","args":["-y","mcp-fetch"],"env":{"K":"V"}},"remote":{"type":"http","url":"https://x"}}}"#,
        )
        .unwrap();
        let mut candidates = Vec::new();
        collect_mcp_candidates(&file, &fs::read_to_string(&file).unwrap(), &mut candidates);
        assert_eq!(candidates.len(), 1, "http 类型应被跳过");
        assert_eq!(candidates[0].name, "fetch");
        assert_eq!(candidates[0].args, vec!["-y", "mcp-fetch"]);
        assert_eq!(candidates[0].env.get("K").map(String::as_str), Some("V"));
    }

    #[test]
    fn plugin_context_lists_skills_and_servers() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_plugin_context(dir.path()).is_none());

        write_skill(&skills_dir(dir.path()), "alpha", "技能 A");
        add_mcp_server(
            dir.path(),
            "fetch",
            "npx",
            &[],
            &std::collections::BTreeMap::new(),
            true,
        )
        .unwrap();
        add_mcp_server(
            dir.path(),
            "disabled",
            "node",
            &[],
            &std::collections::BTreeMap::new(),
            false,
        )
        .unwrap();

        let context = load_plugin_context(dir.path()).unwrap();
        assert!(context.contains("[可用技能]"));
        assert!(context.contains("alpha"));
        assert!(context.contains("[MCP 服务器]"));
        assert!(context.contains("fetch"));
        assert!(!context.contains("disabled"));
    }

    #[test]
    fn zip_install_finds_skill_md() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        // 手工构造 zip：wrapper/SKILL.md
        let bytes: Vec<u8> = {
            let mut buf = std::io::Cursor::new(Vec::new());
            {
                let mut writer = zip::ZipWriter::new(&mut buf);
                let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
                writer.start_file("wrapper/SKILL.md", options).unwrap();
                std::io::Write::write_all(
                    &mut writer,
                    b"---\nname: zip-skill\ndescription: from zip\n---\nbody",
                )
                .unwrap();
                writer.finish().unwrap();
            }
            buf.into_inner()
        };
        let (_temp, root) = unzip_to_temp(&bytes).unwrap();
        let meta = install_skill_from_extracted(workspace, &root, "download").unwrap();
        assert_eq!(meta.name, "zip-skill");
    }
}
