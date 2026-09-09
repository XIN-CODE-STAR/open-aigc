//! 画布工作记忆的检索（RAG）桥接层。
//!
//! 前端无限画布的节点存储在 memory_canvases / memory_nodes（与 agent 侧
//! 旧 canvas_* 表相互独立，后者已废弃）。本模块把画布内容按对话检索、
//! 打分并格式化为 agent 可用的工作记忆上下文：
//! - 规划阶段：以用户消息为 query 检索最相关的画布节点注入提示词；
//! - 图片生成富化：注入画布全量空间上下文；
//! - canvas_search 工具：让 agent 主动查询画布内容。

use std::path::Path;

use crate::adapters::sqlite::memory_canvas_repository::SqliteMemoryCanvasRepository;
use crate::ports::memory_canvas_repository::MemoryCanvasRepository;

const DB_FILE: &str = "aigc-studio.sqlite3";

fn open_repo(workspace_path: &Path) -> Option<SqliteMemoryCanvasRepository> {
    SqliteMemoryCanvasRepository::open(&workspace_path.join(DB_FILE)).ok()
}

fn find_canvas_id(repo: &SqliteMemoryCanvasRepository, conversation_id: &str) -> Option<String> {
    let canvases = repo.list_canvases("default").ok()?;
    canvases
        .into_iter()
        .find(|c| c.name == format!("conv-{conversation_id}"))
        .map(|c| c.id)
}

/// 从 payload_json 提取可检索文本（description / text / summary 等字段拼合）。
fn node_text(payload_json: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(payload_json) else {
        return String::new();
    };
    let mut parts = Vec::new();
    for key in ["description", "text", "summary", "prompt", "ocr_text"] {
        if let Some(text) = value.get(key).and_then(|v| v.as_str()) {
            parts.push(text.to_owned());
        }
    }
    parts.join(" ")
}

/// 把查询拆为可匹配的词项：拉丁词（≥2 字符）与中文二元组。
fn extract_terms(query: &str) -> Vec<String> {
    let lower = query.to_lowercase();
    let mut terms = Vec::new();
    let mut current_cjk = String::new();
    let mut current_word = String::new();

    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            current_word.push(ch);
        } else {
            if current_word.chars().count() >= 2 {
                terms.push(current_word.clone());
            }
            current_word.clear();

            if is_cjk(ch) {
                current_cjk.push(ch);
            } else {
                flush_cjk(&current_cjk, &mut terms);
                current_cjk.clear();
            }
        }
    }
    if current_word.chars().count() >= 2 {
        terms.push(current_word);
    }
    flush_cjk(&current_cjk, &mut terms);
    terms
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF)
}

fn flush_cjk(run: &str, terms: &mut Vec<String>) {
    let chars: Vec<char> = run.chars().collect();
    if chars.len() >= 2 {
        for pair in chars.windows(2) {
            terms.push(pair.iter().collect());
        }
    } else if let Some(single) = chars.first() {
        terms.push(single.to_string());
    }
}

fn score_node(node_text: &str, terms: &[String]) -> f64 {
    let lower = node_text.to_lowercase();
    let mut score = 0.0;
    for term in terms {
        if term.is_empty() {
            continue;
        }
        let mut count = 0;
        let mut start = 0;
        while let Some(pos) = lower[start..].find(term.as_str()) {
            count += 1;
            start += pos + term.len();
        }
        score += count as f64;
    }
    score
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}

fn node_display_text(summary: Option<&str>, payload_json: &str) -> String {
    let payload_text = node_text(payload_json);
    let mut parts = Vec::new();
    if let Some(summary) = summary.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(summary.to_owned());
    }
    if !payload_text.trim().is_empty() && payload_text.trim() != summary.unwrap_or_default() {
        parts.push(truncate_chars(payload_text.trim(), 160));
    }
    parts.join(" — ")
}

/// 加载当前对话画布的工作记忆上下文（agent 用）。
///
/// - `query` 为 None 或空：返回全量画布概览（按类型分组）。
/// - `query` 有值：按词项重合度检索最相关的 `max_nodes` 个节点。
/// 画布不存在或没有可用内容时返回 None。
pub fn load_working_memory_context(
    workspace_path: &Path,
    conversation_id: &str,
    query: Option<&str>,
    max_nodes: usize,
) -> Option<String> {
    let repo = open_repo(workspace_path)?;
    let canvas_id = find_canvas_id(&repo, conversation_id)?;
    let nodes = repo.list_nodes(&canvas_id).ok()?;
    if nodes.is_empty() {
        return None;
    }

    let query_terms = query.map(extract_terms).unwrap_or_default();
    let has_query = query.map(|q| !q.trim().is_empty()).unwrap_or(false);
    let retrieval_mode = has_query && !query_terms.is_empty();

    let mut scored: Vec<(f64, String)> = nodes
        .into_iter()
        .map(|n| {
            let text = node_display_text(n.summary.as_deref(), &n.payload_json);
            let score = if retrieval_mode {
                score_node(&text, &query_terms)
                    + if query_terms
                        .iter()
                        .any(|t| n.node_type.to_lowercase().contains(t.as_str()))
                    {
                        1.0
                    } else {
                        0.0
                    }
            } else {
                1.0
            };
            (score, format!("[{}] {}", n.node_type, text))
        })
        .collect();

    if retrieval_mode {
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.retain(|(score, _)| *score > 0.0);
    }
    if scored.is_empty() {
        return None;
    }
    scored.truncate(max_nodes);

    let body = scored
        .into_iter()
        .map(|(_, text)| format!("- {text}"))
        .collect::<Vec<_>>()
        .join("\n");

    Some(if retrieval_mode {
        format!("[画布工作记忆（与当前请求相关）]\n{body}")
    } else {
        format!("[画布工作记忆]\n{body}")
    })
}

/// 在当前对话画布中检索节点（canvas_search 工具用）。
/// 返回 (节点类型 + 摘要, 得分) 列表，按相关度降序。
pub fn search_nodes(
    workspace_path: &Path,
    conversation_id: &str,
    query: &str,
    limit: usize,
) -> Vec<(String, f64)> {
    let Some(repo) = open_repo(workspace_path) else {
        return Vec::new();
    };
    let Some(canvas_id) = find_canvas_id(&repo, conversation_id) else {
        return Vec::new();
    };
    let Ok(nodes) = repo.list_nodes(&canvas_id) else {
        return Vec::new();
    };
    let terms = extract_terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(String, f64)> = nodes
        .into_iter()
        .map(|n| {
            let text = node_display_text(n.summary.as_deref(), &n.payload_json);
            let full = format!("{} {text}", n.node_type);
            let score = score_node(&full, &terms);
            (format!("[{}] {}", n.node_type, text), score)
        })
        .filter(|(_, score)| *score > 0.0)
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    scored
}

// ── Agent 直接操作画布（P3：Runtime API） ──

/// 确保画布存在，返回 canvas_id。
pub fn ensure_canvas(workspace_path: &Path, conversation_id: &str) -> Option<String> {
    let repo = open_repo(workspace_path)?;
    let canvas_id = find_canvas_id(&repo, conversation_id);
    if let Some(id) = canvas_id {
        return Some(id);
    }
    // 创建画布
    let draft = crate::ports::memory_canvas_repository::CanvasDraft {
        workspace_id: "default".to_owned(),
        name: format!("conv-{conversation_id}"),
        description: None,
    };
    let canvas = repo.create_canvas(draft).ok()?;
    Some(canvas.id)
}

/// Agent 在画布上创建一个便签节点，返回节点 ID。
pub fn add_canvas_note(
    workspace_path: &Path,
    conversation_id: &str,
    flow_x: f64,
    flow_y: f64,
    text: &str,
) -> Option<String> {
    let repo = open_repo(workspace_path)?;
    let canvas_id = ensure_canvas(workspace_path, conversation_id)?;
    let node = repo
        .add_node(crate::ports::memory_canvas_repository::NodeDraft {
            canvas_id,
            node_type: "note".to_owned(),
            position_x: flow_x,
            position_y: flow_y,
            width: None,
            height: None,
            payload_json: serde_json::json!({"text": text, "source": "agent"}).to_string(),
            summary: Some(truncate_chars(text, 50)),
            asset_id: None,
        })
        .ok()?;
    Some(node.id)
}

/// 语义自动连线：将新节点与画布上语义相关的节点建立 reference 边（P4）。
/// 返回自动连接的节点数。
pub fn auto_connect_related(
    workspace_path: &Path,
    conversation_id: &str,
    node_id: &str,
    max_connections: usize,
) -> usize {
    let Some(repo) = open_repo(workspace_path) else {
        return 0;
    };
    let Some(canvas_id) = find_canvas_id(&repo, conversation_id) else {
        return 0;
    };
    let all_nodes = repo.list_nodes(&canvas_id).ok().unwrap_or_default();
    let target = all_nodes.iter().find(|n| n.id == node_id);
    let target_text = match target {
        Some(n) => node_display_text(n.summary.as_deref(), &n.payload_json),
        None => return 0,
    };
    let terms = extract_terms(&target_text);
    if terms.is_empty() {
        return 0;
    }

    // 对已有节点打分（排除自身和已连接的）
    let existing_edges = repo.list_edges(&canvas_id).ok().unwrap_or_default();
    let connected: std::collections::HashSet<String> = existing_edges
        .iter()
        .filter_map(|e| {
            if e.source_node_id == node_id {
                Some(e.target_node_id.clone())
            } else if e.target_node_id == node_id {
                Some(e.source_node_id.clone())
            } else {
                None
            }
        })
        .collect();

    let mut scored: Vec<(f64, &crate::ports::memory_canvas_repository::MemoryNode)> = all_nodes
        .iter()
        .filter(|n| n.id != node_id && !connected.contains(&n.id))
        .map(|n| {
            let text =
                node_display_text(n.summary.as_deref(), &n.payload_json) + " " + &n.node_type;
            (score_node(&text, &terms), n)
        })
        .filter(|(score, _)| *score > 0.0)
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut connected_count = 0;
    for (score, related) in scored.into_iter().take(max_connections) {
        let draft = crate::ports::memory_canvas_repository::EdgeDraft {
            canvas_id: canvas_id.clone(),
            source_node_id: node_id.to_owned(),
            target_node_id: related.id.clone(),
            edge_type: "reference".to_owned(),
            label: Some(format!("自动关联（相关度 {score:.0}）")),
        };
        if repo.add_edge(draft).is_ok() {
            connected_count += 1;
        }
    }
    connected_count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_terms_splits_latin_and_cjk() {
        let terms = extract_terms("去除海报 QRCode 二维码");
        assert!(terms.contains(&"qrcode".to_owned()));
        assert!(terms.contains(&"去除".to_owned()) || terms.contains(&"海报".to_owned()));
    }

    #[test]
    fn working_memory_requires_canvas() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_working_memory_context(dir.path(), "conv-x", None, 5).is_none());
    }

    #[test]
    fn retrieval_ranks_matching_nodes_first() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        let repo = SqliteMemoryCanvasRepository::open(&db).unwrap();
        let canvas = repo
            .create_canvas(crate::ports::memory_canvas_repository::CanvasDraft {
                workspace_id: "default".to_owned(),
                name: "conv-test".to_owned(),
                description: None,
            })
            .unwrap();
        repo.add_node(crate::ports::memory_canvas_repository::NodeDraft {
            canvas_id: canvas.id.clone(),
            node_type: "note".to_owned(),
            position_x: 0.0,
            position_y: 0.0,
            width: None,
            height: None,
            payload_json: r#"{"text":"用户喜欢极简风格的设计"}"#.to_owned(),
            summary: Some("风格偏好".to_owned()),
            asset_id: None,
        })
        .unwrap();
        repo.add_node(crate::ports::memory_canvas_repository::NodeDraft {
            canvas_id: canvas.id.clone(),
            node_type: "image".to_owned(),
            position_x: 100.0,
            position_y: 0.0,
            width: None,
            height: None,
            payload_json: r#"{"description":"山景照片"}"#.to_owned(),
            summary: Some("山景".to_owned()),
            asset_id: None,
        })
        .unwrap();
        drop(repo);

        let ctx = load_working_memory_context(dir.path(), "test", Some("极简风格 设计"), 1)
            .expect("should have working memory");
        assert!(ctx.contains("风格偏好"), "ctx: {ctx}");

        let hits = search_nodes(dir.path(), "test", "山景", 3);
        assert!(!hits.is_empty());
        assert!(hits[0].0.contains("山景"));
    }
}
