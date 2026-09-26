//! 画布工作记忆的检索（RAG）与 Agent Runtime API 桥接层。
//!
//! 前端无限画布的节点存储在 memory_canvases / memory_nodes（与 agent 侧
//! 旧 canvas_* 表相互独立，后者已废弃）。本模块把画布内容按对话检索、
//! 打分并格式化为 agent 可用的工作记忆上下文：
//! - 规划阶段：以用户消息为 query 检索最相关的画布节点注入提示词
//!   （图片节点附带 qwen-vl 视觉分析结果，并注入画布连线关系，总量受限）；
//! - 图片生成富化：注入画布全量空间上下文；
//! - canvas_search / canvas_add_note / canvas_add_image / canvas_update_node /
//!   canvas_auto_layout / canvas_export 工具：让 agent 查询与直接操作画布。

use base64::Engine as _;
use std::collections::HashMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::adapters::embedding_openai::OpenAiCompatibleEmbeddingAdapter;
use crate::adapters::sqlite::memory_canvas_repository::SqliteMemoryCanvasRepository;
use crate::adapters::sqlite::semantic_repository::SqliteSemanticRepository;
use crate::adapters::sqlite::vector_repository::SqliteVectorRepository;
use crate::ports::embedding_port::EmbeddingPort;
use crate::ports::memory_canvas_repository::MemoryCanvasRepository;
use crate::ports::memory_canvas_repository::{EdgeDraft, MemoryNode, NodeDraft};
use crate::ports::semantic_repository::SemanticRepository;
use crate::ports::vector_repository::{
    VectorEntry, VectorMetadata, VectorRepository, VectorSearchRequest,
};

const DB_FILE: &str = "aigc-studio.sqlite3";

/// 注入 agent 上下文的总字符上限（防止超大画布撑爆提示词）。
const MAX_CONTEXT_CHARS: usize = 4000;

/// 检索邻接加权重：一跳（直连）0.35，二跳 0.12。
const NEIGHBOR_BOOST_HOP1: f64 = 0.35;
const NEIGHBOR_BOOST_HOP2: f64 = 0.12;

/// 向量检索得分权重（归一化余弦 0..1 → 与关键词词频同量纲）。
const VECTOR_SCORE_WEIGHT: f64 = 4.0;
/// 嵌入模型名（可用环境变量 AIGC_EMBEDDING_MODEL 覆盖）。
const DEFAULT_EMBEDDING_MODEL: &str = "embedding-3";
/// 单次批量嵌入的节点上限（防止首次索引一次请求过大）。
const EMBED_BATCH_LIMIT: usize = 32;
/// 画布节点向量的 content_type 标识。
const CANVAS_NODE_CONTENT_TYPE: &str = "canvas-node";
/// 向量条目 id 前缀（canvas-node:<node_id>）。
const CANVAS_VECTOR_PREFIX: &str = "canvas-node:";

/// 解析对话可用嵌入器：会话引用的凭据（base_url + 钥匙串密钥）构造
/// OpenAI 兼容适配器。无会话/凭据/密钥时返回 None（检索降级为关键词）。
fn resolve_embedder(
    workspace_path: &Path,
    conversation_id: &str,
) -> Option<OpenAiCompatibleEmbeddingAdapter> {
    use crate::adapters::sqlite::credential_repository::SqliteCredentialRepository;
    use crate::ports::credential_repository::CredentialRepository;

    let db_path = workspace_path.join(DB_FILE);
    let connection = rusqlite::Connection::open(&db_path).ok()?;
    let credential_id: Option<String> = connection
        .query_row(
            "SELECT credential_id FROM agent_conversations WHERE id = ?1",
            [conversation_id],
            |row| row.get(0),
        )
        .ok();

    let mut credential_repo = SqliteCredentialRepository::open(&db_path).ok()?;
    let credential = match credential_id {
        Some(id) => credential_repo.get(&id).ok().flatten(),
        // 会话无凭据引用时回退最近一个启用的凭据
        None => credential_repo.list().ok().and_then(|all| {
            all.into_iter()
                .filter(|c| c.enabled && !c.base_url.is_empty())
                .max_by(|a, b| a.created_at.cmp(&b.created_at))
        }),
    }?;
    if credential.base_url.is_empty() {
        return None;
    }
    let api_key = credential_repo
        .get_secret(&credential.credential_key)
        .ok()?;
    if api_key.is_empty() {
        return None;
    }
    let model = std::env::var("AIGC_EMBEDDING_MODEL")
        .unwrap_or_else(|_| DEFAULT_EMBEDDING_MODEL.to_owned());
    Some(OpenAiCompatibleEmbeddingAdapter::new(
        credential.base_url,
        api_key,
        model,
    ))
}

/// 惰性索引同步 + 查询向量检索：节点文本变化时批量重嵌入，然后按余弦相似度
/// 返回 node_id → 归一化得分（0..1）。任何失败都降级返回 None（关键词检索兜底）。
fn hybrid_vector_scores(
    workspace_path: &Path,
    conversation_id: &str,
    nodes: &[MemoryNode],
    query: &str,
) -> Option<std::collections::HashMap<String, f64>> {
    let embedder = resolve_embedder(workspace_path, conversation_id)?;
    let vector_repo = SqliteVectorRepository::open(workspace_path.join(DB_FILE)).ok()?;

    // 1) 索引同步：文本与已存向量不一致的节点批量重嵌入（限量）
    let mut stale: Vec<&MemoryNode> = Vec::new();
    for node in nodes {
        let text = node_rich_text(node, None);
        if text.trim().is_empty() {
            continue;
        }
        let vector_id = format!("{CANVAS_VECTOR_PREFIX}{}", node.id);
        let needs_index = vector_repo
            .get(&vector_id)
            .ok()
            .flatten()
            .map(|entry| entry.metadata.text != text)
            .unwrap_or(true);
        if needs_index {
            stale.push(node);
        }
    }
    if !stale.is_empty() {
        let batch: Vec<&MemoryNode> = stale.iter().take(EMBED_BATCH_LIMIT).copied().collect();
        let requests: Vec<crate::ports::embedding_port::EmbeddingRequest> = batch
            .iter()
            .map(|node| crate::ports::embedding_port::EmbeddingRequest {
                text: node_rich_text(node, None),
                model: None,
                asset_id: Some(node.id.clone()),
            })
            .collect();
        let results = embedder.embed_batch(&requests).ok()?;
        for (node, result) in batch.iter().zip(results) {
            let vector_id = format!("{CANVAS_VECTOR_PREFIX}{}", node.id);
            vector_repo
                .store(&VectorEntry {
                    id: vector_id,
                    embedding: result.embedding,
                    metadata: VectorMetadata {
                        asset_id: node.id.clone(),
                        content_type: CANVAS_NODE_CONTENT_TYPE.to_owned(),
                        text: node_rich_text(node, None),
                        extra: Some(serde_json::json!({
                            "canvasId": node.canvas_id,
                            "conversationId": conversation_id,
                        })),
                    },
                })
                .ok()?;
        }
    }

    // 2) 查询向量 → 余弦检索（cos -1..1 归一化到 0..1）
    if query.trim().is_empty() {
        return Some(std::collections::HashMap::new());
    }
    let query_result = embedder
        .embed(&crate::ports::embedding_port::EmbeddingRequest {
            text: query.to_owned(),
            model: None,
            asset_id: None,
        })
        .ok()?;
    let hits = vector_repo
        .search(&VectorSearchRequest {
            embedding: query_result.embedding,
            limit: nodes.len().max(1),
            min_score: None,
            asset_ids: None,
            content_type: Some(CANVAS_NODE_CONTENT_TYPE.to_owned()),
        })
        .ok()?;
    let mut scores = std::collections::HashMap::new();
    for hit in hits {
        let normalized = ((hit.score + 1.0) / 2.0).clamp(0.0, 1.0) as f64;
        if let Some(node_id) = hit
            .entry
            .id
            .strip_prefix(CANVAS_VECTOR_PREFIX)
            .map(str::to_owned)
        {
            scores.insert(node_id, normalized);
        }
    }
    Some(scores)
}

/// 上下文中最多注入的连线关系条数。
const MAX_RELATION_LINES: usize = 12;

/// 画布仓储连接缓存：RAG 每轮被调用 2-3 次，每次 open 都要做迁移检查，
/// 缓存后省掉重复开销。上限超过时整体清空（最简单且正确性无损）。
fn canvas_repo_cache() -> &'static Mutex<HashMap<PathBuf, Arc<SqliteMemoryCanvasRepository>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<SqliteMemoryCanvasRepository>>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn semantic_repo_cache() -> &'static Mutex<HashMap<PathBuf, Arc<SqliteSemanticRepository>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<SqliteSemanticRepository>>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

const REPO_CACHE_MAX_ENTRIES: usize = 16;

fn open_repo(workspace_path: &Path) -> Option<Arc<SqliteMemoryCanvasRepository>> {
    let key = workspace_path.join(DB_FILE);
    let mut cache = canvas_repo_cache().lock().ok()?;
    if cache.len() >= REPO_CACHE_MAX_ENTRIES {
        cache.clear();
    }
    if let Some(cached) = cache.get(&key) {
        return Some(cached.clone());
    }
    let repo = Arc::new(SqliteMemoryCanvasRepository::open(&key).ok()?);
    cache.insert(key, repo.clone());
    Some(repo)
}

fn open_semantic_repo(workspace_path: &Path) -> Option<Arc<SqliteSemanticRepository>> {
    let key = workspace_path.join(DB_FILE);
    let mut cache = semantic_repo_cache().lock().ok()?;
    if cache.len() >= REPO_CACHE_MAX_ENTRIES {
        cache.clear();
    }
    if let Some(cached) = cache.get(&key) {
        return Some(cached.clone());
    }
    let connection = rusqlite::Connection::open(&key).ok()?;
    let repo = Arc::new(SqliteSemanticRepository::open(connection));
    cache.insert(key, repo.clone());
    Some(repo)
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

/// 读取 payload_json 中某个字符串字段。
fn payload_str(node: &MemoryNode, key: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(&node.payload_json)
        .ok()?
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::to_owned)
}

/// 图片节点的视觉分析摘要（qwen-vl caption + 标签），未分析或无 asset_id 时为空。
fn node_vision_text(semantic: Option<&SqliteSemanticRepository>, asset_id: Option<&str>) -> String {
    let Some(asset_id) = asset_id.filter(|id| !id.is_empty()) else {
        return String::new();
    };
    let Some(semantic) = semantic else {
        return String::new();
    };
    let Ok(Some(profile)) = semantic.get_profile(asset_id) else {
        return String::new();
    };
    let mut parts = Vec::new();
    if let Some(caption) = profile
        .caption
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        parts.push(caption.to_owned());
    }
    if !profile.tags.is_empty() {
        let tags = profile
            .tags
            .iter()
            .take(8)
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join("、");
        parts.push(format!("标签：{tags}"));
    }
    let joined = parts.join("｜");
    if joined.is_empty() {
        String::new()
    } else {
        truncate_chars(&joined, 120)
    }
}

/// 检索用节点视图：剥离 payload 中的 base64 大对象（dataUrl），
/// 避免图片多的画布在每次检索时把兆级字符串载入内存参与打分。
fn strip_heavy_payload(node: &MemoryNode) -> MemoryNode {
    let mut node = node.clone();
    if let Ok(mut payload) = serde_json::from_str::<serde_json::Value>(&node.payload_json) {
        if let Some(obj) = payload.as_object_mut() {
            if obj.contains_key("dataUrl") {
                obj.insert("dataUrl".to_owned(), serde_json::json!("<base64 已省略>"));
            }
        }
        node.payload_json = payload.to_string();
    }
    node
}

/// 节点展示文本 + 视觉分析（检索与注入共用）。
fn node_rich_text(node: &MemoryNode, semantic: Option<&SqliteSemanticRepository>) -> String {
    let display = node_display_text(node.summary.as_deref(), &node.payload_json);
    let vision = node_vision_text(semantic, node.asset_id.as_deref());
    if vision.is_empty() {
        display
    } else if display.is_empty() {
        format!("（视觉分析：{vision}）")
    } else {
        format!("{display}（视觉分析：{vision}）")
    }
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
///
/// 增强（P3）：
/// - 图片节点附带 qwen-vl 视觉分析结果（artifact_semantic_profiles）；
/// - 附带画布连线关系（"A —[标签]→ B"）；
/// - 总量限制在 [`MAX_CONTEXT_CHARS`] 字符内。
///
/// 画布不存在或没有可用内容时返回 None。
pub fn load_working_memory_context(
    workspace_path: &Path,
    conversation_id: &str,
    query: Option<&str>,
    max_nodes: usize,
) -> Option<String> {
    let repo = open_repo(workspace_path)?;
    let canvas_id = find_canvas_id(&repo, conversation_id)?;
    let mut nodes: Vec<MemoryNode> = repo
        .list_nodes(&canvas_id)
        .ok()?
        .iter()
        .map(strip_heavy_payload)
        .collect();
    if nodes.is_empty() {
        return None;
    }
    let semantic = open_semantic_repo(workspace_path);

    let query_terms = query.map(extract_terms).unwrap_or_default();
    let has_query = query.map(|q| !q.trim().is_empty()).unwrap_or(false);
    let retrieval_mode = has_query && !query_terms.is_empty();
    // 概览模式按 updated_at 倒序：最近编辑的节点优先进入上下文
    if !retrieval_mode {
        nodes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    }

    let mut scored: Vec<(f64, String, String)> = nodes
        .iter()
        .map(|n| {
            let rich = node_rich_text(n, semantic.as_deref());
            let score = if retrieval_mode {
                score_node(&rich, &query_terms)
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
            (score, format!("[{}] {}", n.node_type, rich), n.id.clone())
        })
        .collect();

    if retrieval_mode {
        // 向量语义得分合并（混合检索）：嵌入凭据可用时按余弦相似度加权，
        // 让语义相关但词面不同的内容（"城市夜景" ↔ "赛博朋克"）也能命中。
        // 嵌入不可用（无凭据/接口失败）时优雅降级：跳过向量分量，仅关键词。
        if let Some(vector_scores) = hybrid_vector_scores(
            workspace_path,
            conversation_id,
            &nodes,
            query.unwrap_or_default(),
        ) {
            for (score, _, id) in scored.iter_mut() {
                if let Some(vector_score) = vector_scores.get(id) {
                    *score += VECTOR_SCORE_WEIGHT * vector_score;
                }
            }
        }
        // 邻接加权：与命中节点直连的内容按 35% 比例继承得分（一跳，不级联），
        // 让通过连线关联的上下文（如参考图的描述便签）优先进入注入内容。
        let edges = repo.list_edges(&canvas_id).ok().unwrap_or_default();
        let mut adjacency: std::collections::HashMap<&str, Vec<&str>> =
            std::collections::HashMap::new();
        for e in &edges {
            adjacency
                .entry(e.source_node_id.as_str())
                .or_default()
                .push(e.target_node_id.as_str());
            adjacency
                .entry(e.target_node_id.as_str())
                .or_default()
                .push(e.source_node_id.as_str());
        }
        if !adjacency.is_empty() {
            // 两阶段：先收集加成（基于原始得分），再统一应用，避免顺序级联
            // 一跳 0.35，二跳 0.12（图上两步可达的相关内容也能进入上下文）
            let id_to_index: std::collections::HashMap<&str, usize> = nodes
                .iter()
                .enumerate()
                .map(|(i, n)| (n.id.as_str(), i))
                .collect();
            let mut boosts = vec![0.0f64; scored.len()];
            for (i, n) in nodes.iter().enumerate() {
                let base = scored[i].0;
                if base <= 0.0 {
                    continue;
                }
                if let Some(neighbors) = adjacency.get(n.id.as_str()) {
                    for &hop1 in neighbors {
                        let Some(&j) = id_to_index.get(hop1) else {
                            continue;
                        };
                        boosts[j] += NEIGHBOR_BOOST_HOP1 * base;
                        // 二跳：邻居的邻居（排除命中节点自身）
                        if let Some(hop2_neighbors) = adjacency.get(hop1) {
                            for &hop2 in hop2_neighbors {
                                if hop2 == n.id.as_str() {
                                    continue;
                                }
                                if let Some(&k) = id_to_index.get(hop2) {
                                    boosts[k] += NEIGHBOR_BOOST_HOP2 * base;
                                }
                            }
                        }
                    }
                }
            }
            for ((score, _, _), boost) in scored.iter_mut().zip(boosts.iter()) {
                *score += *boost;
            }
        }
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.retain(|(score, _, _)| *score > 0.0);
    }
    if scored.is_empty() {
        return None;
    }
    scored.truncate(max_nodes);

    // 注入集合内的节点 id（用于连线关系优先级）
    let injected_ids: std::collections::HashSet<String> =
        scored.iter().map(|(_, _, id)| id.clone()).collect();

    let mut body = scored
        .into_iter()
        .map(|(_, text, _)| format!("- {text}"))
        .collect::<Vec<_>>()
        .join("\n");

    // 画布连线关系：让 agent 感知节点之间的引用/依赖结构。
    // 注入集合内部的连线优先展示（相关性最高），其余按建库顺序补足。
    let edges = repo.list_edges(&canvas_id).ok().unwrap_or_default();
    if !edges.is_empty() {
        let short_label = |id: &str| {
            nodes
                .iter()
                .find(|n| n.id == id)
                .map(|n| {
                    let display = node_display_text(n.summary.as_deref(), &n.payload_json);
                    truncate_chars(&display, 20)
                })
                .unwrap_or_else(|| "未知节点".to_owned())
        };
        let (internal, external): (Vec<_>, Vec<_>) = edges.iter().partition(|e| {
            injected_ids.contains(&e.source_node_id) && injected_ids.contains(&e.target_node_id)
        });
        let relations = internal
            .iter()
            .chain(external.iter())
            .take(MAX_RELATION_LINES)
            .map(|e| {
                let label = e.label.as_deref().unwrap_or(e.edge_type.as_str());
                format!(
                    "- {} —[{label}]→ {}",
                    short_label(&e.source_node_id),
                    short_label(&e.target_node_id)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        body.push_str(&format!("\n[画布连线关系]\n{relations}"));
    }

    let header = if retrieval_mode {
        "[画布工作记忆（与当前请求相关）]"
    } else {
        "[画布工作记忆]"
    };
    Some(format!(
        "{header}\n{}",
        truncate_chars(&body, MAX_CONTEXT_CHARS)
    ))
}

/// 在当前对话画布中检索节点（canvas_search 工具用）。
/// 返回 (节点 ID, 展示文本, 得分) 列表，按相关度降序；ID 供 canvas_update_node 使用。
pub fn search_nodes(
    workspace_path: &Path,
    conversation_id: &str,
    query: &str,
    limit: usize,
) -> Vec<(String, String, f64)> {
    let Some(repo) = open_repo(workspace_path) else {
        return Vec::new();
    };
    let Some(canvas_id) = find_canvas_id(&repo, conversation_id) else {
        return Vec::new();
    };
    let Ok(nodes) = repo.list_nodes(&canvas_id) else {
        return Vec::new();
    };
    let nodes: Vec<MemoryNode> = nodes.iter().map(strip_heavy_payload).collect();
    let semantic = open_semantic_repo(workspace_path);
    let terms = extract_terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(String, String, f64)> = nodes
        .iter()
        .map(|n| {
            let rich = node_rich_text(n, semantic.as_deref());
            let full = format!("{} {rich}", n.node_type);
            let score = score_node(&full, &terms);
            (n.id.clone(), format!("[{}] {}", n.node_type, rich), score)
        })
        .filter(|(_, _, score)| *score > 0.0)
        .collect();
    scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
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
        .add_node(NodeDraft {
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

/// Agent 在画布上创建一个图片节点（canvas_add_image 工具）。
///
/// 同一结果（task_id）或同一图片地址已存在节点时直接复用，避免重复创建。
pub fn add_canvas_image(
    workspace_path: &Path,
    conversation_id: &str,
    image_url: &str,
    description: Option<&str>,
    flow_x: Option<f64>,
    flow_y: Option<f64>,
    task_id: Option<&str>,
) -> Option<String> {
    let repo = open_repo(workspace_path)?;
    let canvas_id = ensure_canvas(workspace_path, conversation_id)?;
    let nodes = repo.list_nodes(&canvas_id).ok().unwrap_or_default();

    for node in &nodes {
        if payload_str(node, "imageUrl").as_deref() == Some(image_url) {
            return Some(node.id.clone());
        }
        if task_id.is_some() && payload_str(node, "taskId").as_deref() == task_id {
            return Some(node.id.clone());
        }
    }

    let (x, y) = match (flow_x, flow_y) {
        (Some(x), Some(y)) => (x, y),
        _ => match nodes.last() {
            Some(last) => (last.position_x + 280.0, last.position_y),
            None => (40.0, 40.0),
        },
    };

    let mut payload = serde_json::json!({ "imageUrl": image_url, "source": "agent" });
    if let Some(desc) = description.map(str::trim).filter(|s| !s.is_empty()) {
        payload["description"] = serde_json::Value::String(desc.to_owned());
    }
    if let Some(tid) = task_id {
        payload["taskId"] = serde_json::Value::String(tid.to_owned());
    }
    let summary = description
        .map(|d| truncate_chars(d.trim(), 50))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "生成图片".to_owned());

    let node = repo
        .add_node(NodeDraft {
            canvas_id,
            node_type: "image".to_owned(),
            position_x: x,
            position_y: y,
            width: None,
            height: None,
            payload_json: payload.to_string(),
            summary: Some(summary),
            asset_id: None,
        })
        .ok()?;
    Some(node.id)
}

/// 建立指定两节点间的 reference 连线；已存在同向连线时不重复创建。
pub fn connect_nodes(
    workspace_path: &Path,
    conversation_id: &str,
    source_id: &str,
    target_id: &str,
    label: Option<&str>,
) -> Option<String> {
    if source_id == target_id {
        return None;
    }
    let repo = open_repo(workspace_path)?;
    let canvas_id = find_canvas_id(&repo, conversation_id)?;
    let edges = repo.list_edges(&canvas_id).ok().unwrap_or_default();
    if edges.iter().any(|e| {
        e.source_node_id == source_id && e.target_node_id == target_id && e.edge_type == "reference"
    }) {
        return None;
    }
    let edge = repo
        .add_edge(EdgeDraft {
            canvas_id,
            source_node_id: source_id.to_owned(),
            target_node_id: target_id.to_owned(),
            edge_type: "reference".to_owned(),
            label: label.map(str::to_owned),
        })
        .ok()?;
    Some(edge.id)
}

/// 更新画布节点内容（canvas_update_node 工具）：便签文本 / 节点颜色。
/// 返回更新后的节点 ID；节点不存在时为 None。
pub fn update_canvas_node(
    workspace_path: &Path,
    conversation_id: &str,
    node_id: &str,
    text: Option<&str>,
    color: Option<&str>,
) -> Option<String> {
    let repo = open_repo(workspace_path)?;
    let canvas_id = find_canvas_id(&repo, conversation_id)?;
    let nodes = repo.list_nodes(&canvas_id).ok()?;
    let node = nodes.into_iter().find(|n| n.id == node_id)?;

    let mut payload: serde_json::Value =
        serde_json::from_str(&node.payload_json).unwrap_or_else(|_| serde_json::json!({}));
    let mut new_summary = node.summary.clone();
    if let Some(t) = text.map(str::trim).filter(|s| !s.is_empty()) {
        payload["text"] = serde_json::Value::String(t.to_owned());
        new_summary = Some(truncate_chars(t, 50));
    }
    if let Some(c) = color.map(str::trim).filter(|s| !s.is_empty()) {
        payload["color"] = serde_json::Value::String(c.to_owned());
    }
    let payload_json = payload.to_string();
    if payload_json == node.payload_json && new_summary == node.summary {
        return Some(node.id);
    }

    repo.update_node(
        &node.id,
        NodeDraft {
            canvas_id: node.canvas_id.clone(),
            node_type: node.node_type.clone(),
            position_x: node.position_x,
            position_y: node.position_y,
            width: node.width,
            height: node.height,
            payload_json,
            summary: new_summary,
            asset_id: node.asset_id.clone(),
        },
    )
    .ok()
    .map(|_| node.id)
}

/// 自动整理画布（canvas_auto_layout 工具）：按类型分组排成竖列。
/// 返回发生位移的节点数。
pub fn auto_layout_canvas(workspace_path: &Path, conversation_id: &str) -> usize {
    let Some(repo) = open_repo(workspace_path) else {
        return 0;
    };
    let Some(canvas_id) = find_canvas_id(&repo, conversation_id) else {
        return 0;
    };
    let nodes = repo.list_nodes(&canvas_id).ok().unwrap_or_default();
    if nodes.is_empty() {
        return 0;
    }

    const ORDER: [&str; 6] = ["upload", "image", "note", "video", "document", "fact"];
    let mut groups: Vec<(String, Vec<&MemoryNode>)> = Vec::new();
    for node in &nodes {
        match groups.iter_mut().find(|(t, _)| *t == node.node_type) {
            Some((_, list)) => list.push(node),
            None => groups.push((node.node_type.clone(), vec![node])),
        }
    }
    groups.sort_by_key(|(node_type, _)| {
        ORDER
            .iter()
            .position(|t| t == node_type)
            .unwrap_or(usize::MAX)
    });

    let mut moved = 0;
    for (column, (_, group)) in groups.iter().enumerate() {
        for (row, node) in group.iter().enumerate() {
            let x = 60.0 + column as f64 * 280.0;
            let y = 60.0 + row as f64 * 260.0;
            if (node.position_x - x).abs() < f64::EPSILON
                && (node.position_y - y).abs() < f64::EPSILON
            {
                continue;
            }
            let draft = NodeDraft {
                canvas_id: canvas_id.clone(),
                node_type: node.node_type.clone(),
                position_x: x,
                position_y: y,
                width: node.width,
                height: node.height,
                payload_json: node.payload_json.clone(),
                summary: node.summary.clone(),
                asset_id: node.asset_id.clone(),
            };
            if repo.update_node(&node.id, draft).is_ok() {
                moved += 1;
            }
        }
    }
    moved
}

/// 导出画布 JSON（canvas_export 工具）：节点 + 连线。
/// payload 中的 dataUrl（base64 大对象）被省略，避免撑爆 agent 上下文。
pub fn export_canvas(workspace_path: &Path, conversation_id: &str) -> Option<serde_json::Value> {
    let repo = open_repo(workspace_path)?;
    let canvas_id = find_canvas_id(&repo, conversation_id)?;
    let nodes = repo.list_nodes(&canvas_id).ok()?;
    let edges = repo.list_edges(&canvas_id).ok().unwrap_or_default();

    let node_values: Vec<serde_json::Value> = nodes
        .iter()
        .map(|n| {
            let mut payload: serde_json::Value =
                serde_json::from_str(&n.payload_json).unwrap_or_else(|_| serde_json::json!({}));
            if let Some(obj) = payload.as_object_mut() {
                if obj.contains_key("dataUrl") {
                    obj.insert("dataUrl".to_owned(), serde_json::json!("<data URL 已省略>"));
                }
            }
            serde_json::json!({
                "id": n.id,
                "type": n.node_type,
                "position": { "x": n.position_x, "y": n.position_y },
                "summary": n.summary,
                "assetId": n.asset_id,
                "payload": payload,
            })
        })
        .collect();

    let edge_values: Vec<serde_json::Value> = edges
        .iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "source": e.source_node_id,
                "target": e.target_node_id,
                "type": e.edge_type,
                "label": e.label,
            })
        })
        .collect();

    Some(serde_json::json!({
        "canvasId": canvas_id,
        "nodeCount": nodes.len(),
        "edgeCount": edges.len(),
        "nodes": node_values,
        "edges": edge_values,
    }))
}

/// 依据上下文推导连线标签：图片间为「风格参考」，与用户消息重合为「内容补充」。
fn derive_edge_label(target_type: &str, related: &MemoryNode, user_overlap: f64) -> String {
    let is_image = |t: &str| t == "image" || t == "upload";
    if is_image(target_type) && is_image(related.node_type.as_str()) {
        "风格参考".to_owned()
    } else if user_overlap > 0.0 {
        "内容补充".to_owned()
    } else {
        "语义相关".to_owned()
    }
}

/// 语义自动连线：将新节点与画布上语义相关的节点建立 reference 边。
///
/// P3 增强：
/// - 结合用户当前消息一起判断关联（不只是节点自身文本）；
/// - 候选包含 image 节点（命中 qwen-vl 视觉分析文本）；
/// - 边标签从上下文推导（风格参考 / 内容补充 / 语义相关）。
///
/// 返回自动连接的节点数。
pub fn auto_connect_related(
    workspace_path: &Path,
    conversation_id: &str,
    node_id: &str,
    max_connections: usize,
    user_message: Option<&str>,
) -> usize {
    let Some(repo) = open_repo(workspace_path) else {
        return 0;
    };
    let Some(canvas_id) = find_canvas_id(&repo, conversation_id) else {
        return 0;
    };
    let all_nodes = repo.list_nodes(&canvas_id).ok().unwrap_or_default();
    let Some(target) = all_nodes.iter().find(|n| n.id == node_id) else {
        return 0;
    };
    let semantic = open_semantic_repo(workspace_path);
    let target_rich = node_rich_text(target, semantic.as_deref());
    let mut terms = extract_terms(&target_rich);
    let user_terms = user_message.map(extract_terms).unwrap_or_default();
    terms.extend(user_terms.iter().cloned());
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

    let mut scored: Vec<(f64, f64, &MemoryNode)> = all_nodes
        .iter()
        .filter(|n| n.id != node_id && !connected.contains(&n.id))
        .map(|n| {
            let rich = node_rich_text(n, semantic.as_deref());
            let text = format!("{} {rich}", n.node_type);
            let semantic_score = score_node(&text, &terms);
            let user_overlap = score_node(&text, &user_terms);
            (semantic_score + 0.5 * user_overlap, user_overlap, n)
        })
        .filter(|(score, _, _)| *score > 0.0)
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut connected_count = 0;
    for (_total_score, user_overlap, related) in scored.into_iter().take(max_connections) {
        let label = derive_edge_label(target.node_type.as_str(), related, user_overlap);
        let draft = EdgeDraft {
            canvas_id: canvas_id.clone(),
            source_node_id: node_id.to_owned(),
            target_node_id: related.id.clone(),
            edge_type: "reference".to_owned(),
            label: Some(label),
        };
        if repo.add_edge(draft).is_ok() {
            connected_count += 1;
        }
    }
    connected_count
}

/// 删除指定对话的画布（对话删除时级联，软删）。
pub fn delete_canvas_by_conversation(workspace_path: &Path, conversation_id: &str) -> bool {
    let Some(repo) = open_repo(workspace_path) else {
        return false;
    };
    let Some(canvas_id) = find_canvas_id(&repo, conversation_id) else {
        return false;
    };
    repo.delete_canvas(&canvas_id).is_ok()
}

/// 清理孤儿画布（对应对话已被删除的历史遗留，软删）。
/// 返回清理数量。启动时调用一次。
pub fn cleanup_orphan_canvases(workspace_path: &Path) -> usize {
    let Some(repo) = open_repo(workspace_path) else {
        return 0;
    };
    let Ok(canvases) = repo.list_canvases("default") else {
        return 0;
    };
    let Ok(connection) = rusqlite::Connection::open(workspace_path.join(DB_FILE)) else {
        return 0;
    };
    let mut cleaned = 0;
    for canvas in canvases {
        let Some(conversation_id) = canvas.name.strip_prefix("conv-") else {
            continue;
        };
        let exists = connection
            .query_row(
                "SELECT 1 FROM agent_conversations WHERE id = ?1",
                [conversation_id],
                |row| row.get::<_, i64>(0),
            )
            .is_ok();
        if !exists && repo.delete_canvas(&canvas.id).is_ok() {
            cleaned += 1;
        }
    }
    if cleaned > 0 {
        eprintln!("[Canvas] cleaned {cleaned} orphaned canvas(es)");
    }
    cleaned
}

/// 收集 Agent 写入、尚未沉淀到长期记忆的便签（结论/偏好类内容）。
/// 返回 (node_id, 可读文本) 列表；非 Agent 便签与已同步的跳过。
pub fn collect_unsynced_agent_notes(
    workspace_path: &Path,
    conversation_id: &str,
) -> Vec<(String, String)> {
    let Some(repo) = open_repo(workspace_path) else {
        return Vec::new();
    };
    let Some(canvas_id) = find_canvas_id(&repo, conversation_id) else {
        return Vec::new();
    };
    let Ok(nodes) = repo.list_nodes(&canvas_id) else {
        return Vec::new();
    };
    nodes
        .iter()
        .filter(|n| n.node_type == "note")
        .filter_map(|n| {
            let payload = serde_json::from_str::<serde_json::Value>(&n.payload_json).ok()?;
            if payload.get("source").and_then(|v| v.as_str()) != Some("agent") {
                return None;
            }
            if payload.get("syncedToMemory").and_then(|v| v.as_bool()) == Some(true) {
                return None;
            }
            // 占位便签（用户还没编辑过）不沉淀
            let payload_text = payload
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if n.summary.as_deref() == Some("双击编辑便签") || payload_text == "双击编辑便签"
            {
                return None;
            }
            let text = n.summary.clone().or_else(|| {
                payload
                    .get("text")
                    .and_then(|v| v.as_str().map(str::to_owned))
            })?;
            if text.trim().is_empty() {
                return None;
            }
            Some((n.id.clone(), text))
        })
        .collect()
}

/// 标记便签已沉淀到长期记忆（写入 payload.syncedToMemory，幂等）。
pub fn mark_agent_notes_synced(workspace_path: &Path, conversation_id: &str, ids: &[String]) {
    let Some(repo) = open_repo(workspace_path) else {
        return;
    };
    let Some(canvas_id) = find_canvas_id(&repo, conversation_id) else {
        return;
    };
    let Ok(nodes) = repo.list_nodes(&canvas_id) else {
        return;
    };
    for node in nodes.iter().filter(|n| ids.contains(&n.id)) {
        let mut payload: serde_json::Value =
            serde_json::from_str(&node.payload_json).unwrap_or_else(|_| serde_json::json!({}));
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("syncedToMemory".to_owned(), serde_json::json!(true));
        }
        let draft = NodeDraft {
            canvas_id: node.canvas_id.clone(),
            node_type: node.node_type.clone(),
            position_x: node.position_x,
            position_y: node.position_y,
            width: node.width,
            height: node.height,
            payload_json: payload.to_string(),
            summary: node.summary.clone(),
            asset_id: node.asset_id.clone(),
        };
        let _ = repo.update_node(&node.id, draft);
    }
}

/// 参考图定向上下文（图片生成用）：按 dataUrl/imageUrl 找到参考图对应的画布节点，
/// 返回该节点与其连线邻居的上下文——参考图节点自身连着的描述/约束就是本次生成的直接依据。
pub fn load_reference_context(
    workspace_path: &Path,
    conversation_id: &str,
    reference_image: &str,
) -> Option<String> {
    let reference = reference_image.trim();
    if reference.is_empty() {
        return None;
    }
    let repo = open_repo(workspace_path)?;
    let canvas_id = find_canvas_id(&repo, conversation_id)?;
    let nodes: Vec<MemoryNode> = repo
        .list_nodes(&canvas_id)
        .ok()?
        .iter()
        .map(strip_heavy_payload)
        .collect();
    let anchor = nodes.iter().find(|n| {
        payload_str(n, "dataUrl").as_deref() == Some(reference)
            || payload_str(n, "imageUrl").as_deref() == Some(reference)
    })?;
    let semantic = open_semantic_repo(workspace_path);

    let mut lines = vec![format!(
        "- [参考图本体] [{}] {}",
        anchor.node_type,
        node_rich_text(anchor, semantic.as_deref())
    )];

    let edges = repo.list_edges(&canvas_id).ok().unwrap_or_default();
    for edge in edges
        .iter()
        .filter(|e| e.source_node_id == anchor.id || e.target_node_id == anchor.id)
        .take(5)
    {
        let other_id = if edge.source_node_id == anchor.id {
            &edge.target_node_id
        } else {
            &edge.source_node_id
        };
        if let Some(other) = nodes.iter().find(|n| n.id == *other_id) {
            let label = edge.label.as_deref().unwrap_or(edge.edge_type.as_str());
            lines.push(format!(
                "- [连线：{label}] {}",
                node_rich_text(other, semantic.as_deref())
            ));
        }
    }

    Some(format!(
        "[参考图关联上下文]
{}",
        lines.join(
            "
"
        )
    ))
}

/// 生成任务提交时立即在画布挂 pending 占位节点（同步/异步共用）。
///
/// - 记录 taskId（完成侧按它定位补全）；
/// - 参考图能匹配到画布节点时，记录 referenceNodeId 并立即连「风格参考」
///   （生成过程中参考图与任务在画布上即可视化关联）；
/// - 已有同 taskId 节点时幂等返回。
pub fn attach_generation_task(
    workspace_path: &Path,
    conversation_id: &str,
    task_id: &str,
    prompt: &str,
    reference_image: Option<&str>,
) -> Option<String> {
    let repo = open_repo(workspace_path)?;
    let canvas_id = ensure_canvas(workspace_path, conversation_id)?;
    let nodes = repo.list_nodes(&canvas_id).ok().unwrap_or_default();
    if let Some(existing) = nodes
        .iter()
        .find(|n| payload_str(n, "taskId").as_deref() == Some(task_id))
    {
        return Some(existing.id.clone());
    }

    let reference_node_id = reference_image
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .and_then(|reference| {
            nodes
                .iter()
                .find(|n| {
                    payload_str(n, "dataUrl").as_deref() == Some(reference)
                        || payload_str(n, "imageUrl").as_deref() == Some(reference)
                })
                .map(|n| n.id.clone())
        });

    let mut payload = serde_json::json!({
        "prompt": prompt,
        "status": "pending",
        "source": "agent",
        "taskId": task_id,
    });
    if let Some(reference_id) = &reference_node_id {
        payload["referenceNodeId"] = serde_json::Value::String(reference_id.clone());
    }
    let summary = {
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            "生成中…".to_owned()
        } else {
            truncate_chars(trimmed, 50)
        }
    };
    let position_x = nodes.last().map(|n| n.position_x + 280.0).unwrap_or(40.0);
    let position_y = nodes.last().map(|n| n.position_y).unwrap_or(40.0);
    let node = repo
        .add_node(NodeDraft {
            canvas_id: canvas_id.clone(),
            node_type: "image".to_owned(),
            position_x,
            position_y,
            width: None,
            height: None,
            payload_json: payload.to_string(),
            summary: Some(summary),
            asset_id: None,
        })
        .ok()?;

    if let Some(reference_id) = &reference_node_id {
        let _ = repo.add_edge(EdgeDraft {
            canvas_id: canvas_id.clone(),
            source_node_id: reference_id.clone(),
            target_node_id: node.id.clone(),
            edge_type: "reference".to_owned(),
            label: Some("风格参考".to_owned()),
        });
    }
    Some(node.id)
}

/// 生成完成钩子（异步路径）：按 taskId 跨画布定位占位节点并补全图片。
/// 找不到占位节点（任务不来自画布/Agent）时静默返回 None。
pub fn complete_generation_task_by_task_id(
    workspace_path: &Path,
    task_id: &str,
    image_url: &str,
) -> Option<String> {
    let repo = open_repo(workspace_path)?;
    let canvases = repo.list_canvases("default").ok()?;
    for canvas in canvases {
        let Ok(nodes) = repo.list_nodes(&canvas.id) else {
            continue;
        };
        let Some(node) = nodes
            .iter()
            .find(|n| payload_str(n, "taskId").as_deref() == Some(task_id))
        else {
            continue;
        };

        let mut payload: serde_json::Value =
            serde_json::from_str(&node.payload_json).unwrap_or_else(|_| serde_json::json!({}));
        let persisted_data_url = image_source_to_data_url(image_url);
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("imageUrl".to_owned(), serde_json::json!(image_url));
            obj.insert("status".to_owned(), serde_json::json!("succeeded"));
            if let Some(data_url) = &persisted_data_url {
                obj.insert("dataUrl".to_owned(), serde_json::json!(data_url));
            }
        }
        let draft = NodeDraft {
            canvas_id: node.canvas_id.clone(),
            node_type: node.node_type.clone(),
            position_x: node.position_x,
            position_y: node.position_y,
            width: node.width,
            height: node.height,
            payload_json: payload.to_string(),
            summary: node.summary.clone(),
            asset_id: node.asset_id.clone(),
        };
        let _ = repo.update_node(&node.id, draft);

        // attach 阶段已记录 referenceNodeId：确保参考连线存在（幂等）
        if let Some(reference_id) = payload_str(node, "referenceNodeId") {
            if reference_id != node.id {
                let edges = repo.list_edges(&canvas.id).ok().unwrap_or_default();
                let already = edges.iter().any(|e| {
                    e.source_node_id == reference_id
                        && e.target_node_id == node.id
                        && e.edge_type == "reference"
                });
                if !already {
                    let _ = repo.add_edge(EdgeDraft {
                        canvas_id: canvas.id.clone(),
                        source_node_id: reference_id,
                        target_node_id: node.id.clone(),
                        edge_type: "reference".to_owned(),
                        label: Some("风格参考".to_owned()),
                    });
                }
            }
        }
        return Some(node.id.clone());
    }
    None
}

/// 单次下载上限（生成图片一般 <5MB）。
const IMAGE_DOWNLOAD_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// 从字节签名推断图片 MIME。
fn sniff_image_mime(bytes: &[u8]) -> Option<&'static str> {
    let png = [0x89u8, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    let jpeg = [0xFFu8, 0xD8, 0xFF];
    if bytes.len() >= 8 && bytes[0..8] == png {
        Some("image/png")
    } else if bytes.len() >= 3 && bytes[0..3] == jpeg {
        Some("image/jpeg")
    } else if bytes.len() >= 12
        && bytes[0..4] == [0x52u8, 0x49, 0x46, 0x46]
        && bytes[8..12] == [0x57u8, 0x45, 0x42, 0x50]
    {
        Some("image/webp")
    } else if bytes.len() >= 4 && bytes[0..4] == [0x47u8, 0x49, 0x46, 0x38] {
        Some("image/gif")
    } else {
        None
    }
}

fn mime_from_extension(path: &str) -> Option<&'static str> {
    let lower = path.to_lowercase();
    if lower.ends_with(".png") {
        Some("image/png")
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        Some("image/jpeg")
    } else if lower.ends_with(".webp") {
        Some("image/webp")
    } else if lower.ends_with(".gif") {
        Some("image/gif")
    } else {
        None
    }
}

/// 把图片来源（http/https URL 或本地文件路径）转为 data URL，
/// 使画布节点摆脱 CDN 签名 URL 的时效限制。失败返回 None（调用方保留原 URL）。
pub fn image_source_to_data_url(source: &str) -> Option<String> {
    let trimmed = source.trim();
    let (bytes, mime): (Vec<u8>, Option<&str>) =
        if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
            let response = ureq::get(trimmed)
                .timeout(std::time::Duration::from_secs(30))
                .call()
                .ok()?;
            if response.status() >= 400 {
                return None;
            }
            let content_type = response.header("content-type").map(str::to_owned);
            let mut bytes = Vec::new();
            response
                .into_reader()
                .take(IMAGE_DOWNLOAD_MAX_BYTES)
                .read_to_end(&mut bytes)
                .ok()?;
            let sniffed = sniff_image_mime(&bytes);
            let mime = content_type
                .filter(|ct| ct.starts_with("image/"))
                .and_then(|ct| ct.split(';').next().map(str::to_owned))
                .or_else(|| sniffed.map(str::to_owned))?;
            (bytes, Some(Box::leak(mime.into_boxed_str())))
        } else if !trimmed.contains("://") {
            let bytes = std::fs::read(trimmed).ok()?;
            let mime = mime_from_extension(trimmed).or_else(|| sniff_image_mime(&bytes));
            (bytes, mime)
        } else {
            return None;
        };
    if bytes.is_empty() {
        return None;
    }
    let mime = mime.unwrap_or("image/png");
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// 图片生成完成后（同步出图路径）：确保画布上存在该结果的图片节点，
/// 并在画布上能定位到参考图节点时建立「风格参考」连线。
///
/// 复用策略（避免重复节点）：
/// 1. 已有 payload.taskId 匹配的节点 → 仅补写 imageUrl / status；
/// 2. 已有同 prompt 的 pending 占位图片节点（前端面板打开时创建）→ 原地补全；
/// 3. 否则新建 image 节点。
///
/// 返回 (节点 ID, 是否建立了参考连线)。
pub fn ensure_generation_image_node(
    workspace_path: &Path,
    conversation_id: &str,
    task_id: &str,
    image_url: &str,
    prompt: &str,
    reference_image: Option<&str>,
) -> Option<(String, bool)> {
    let repo = open_repo(workspace_path)?;
    let canvas_id = ensure_canvas(workspace_path, conversation_id)?;
    let nodes = repo.list_nodes(&canvas_id).ok().unwrap_or_default();

    // CDN 签名 URL 会过期：立即把图片固化为 dataUrl 随节点持久化
    let persisted_data_url = image_source_to_data_url(image_url);

    let existing = nodes
        .iter()
        .find(|n| payload_str(n, "taskId").as_deref() == Some(task_id))
        .map(|n| n.id.clone())
        .or_else(|| {
            // 前端面板打开时为生成任务创建的 pending 占位节点（同 prompt、尚无图片）
            nodes
                .iter()
                .find(|n| {
                    n.node_type == "image"
                        && payload_str(n, "status").as_deref() == Some("pending")
                        && payload_str(n, "imageUrl").is_none()
                        && payload_str(n, "prompt").as_deref() == Some(prompt)
                })
                .map(|n| n.id.clone())
        });

    let node_id = match existing {
        Some(id) => {
            let node = nodes.iter().find(|n| n.id == id)?;
            let mut payload: serde_json::Value =
                serde_json::from_str(&node.payload_json).unwrap_or_else(|_| serde_json::json!({}));
            if let Some(obj) = payload.as_object_mut() {
                obj.insert("imageUrl".to_owned(), serde_json::json!(image_url));
                obj.insert("status".to_owned(), serde_json::json!("succeeded"));
                obj.insert("taskId".to_owned(), serde_json::json!(task_id));
                if let Some(data_url) = &persisted_data_url {
                    obj.insert("dataUrl".to_owned(), serde_json::json!(data_url));
                }
            }
            let draft = NodeDraft {
                canvas_id: node.canvas_id.clone(),
                node_type: node.node_type.clone(),
                position_x: node.position_x,
                position_y: node.position_y,
                width: node.width,
                height: node.height,
                payload_json: payload.to_string(),
                summary: node.summary.clone(),
                asset_id: node.asset_id.clone(),
            };
            // 更新失败（极少发生）时仍复用旧节点 id，后续连线照常
            let _ = repo.update_node(&id, draft);
            Some(id)
        }
        None => {
            let summary = {
                let trimmed = prompt.trim();
                if trimmed.is_empty() {
                    "生成图片".to_owned()
                } else {
                    truncate_chars(trimmed, 50)
                }
            };
            let position_x = nodes.last().map(|n| n.position_x + 280.0).unwrap_or(40.0);
            let position_y = nodes.last().map(|n| n.position_y).unwrap_or(40.0);
            repo.add_node(NodeDraft {
                canvas_id: canvas_id.clone(),
                node_type: "image".to_owned(),
                position_x,
                position_y,
                width: None,
                height: None,
                payload_json: {
                    let mut payload = serde_json::json!({
                        "imageUrl": image_url,
                        "prompt": prompt,
                        "status": "succeeded",
                        "source": "agent",
                        "taskId": task_id,
                    });
                    if let Some(data_url) = &persisted_data_url {
                        payload["dataUrl"] = serde_json::Value::String(data_url.clone());
                    }
                    payload.to_string()
                },
                summary: Some(summary),
                asset_id: None,
            })
            .ok()
            .map(|n| n.id)
        }
    }?;

    // 参考图连线：画布上 payload.dataUrl / imageUrl 与参考图一致的节点
    let mut reference_connected = false;
    if let Some(reference) = reference_image.map(str::trim).filter(|s| !s.is_empty()) {
        let reference_node = nodes.iter().find(|n| {
            payload_str(n, "dataUrl").as_deref() == Some(reference)
                || payload_str(n, "imageUrl").as_deref() == Some(reference)
        });
        if let Some(ref_node) = reference_node {
            if ref_node.id != node_id {
                let edges = repo.list_edges(&canvas_id).ok().unwrap_or_default();
                let already = edges.iter().any(|e| {
                    e.source_node_id == ref_node.id
                        && e.target_node_id == node_id
                        && e.edge_type == "reference"
                });
                if !already {
                    reference_connected = repo
                        .add_edge(EdgeDraft {
                            canvas_id,
                            source_node_id: ref_node.id.clone(),
                            target_node_id: node_id.clone(),
                            edge_type: "reference".to_owned(),
                            label: Some("风格参考".to_owned()),
                        })
                        .is_ok();
                }
            }
        }
    }

    Some((node_id, reference_connected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::common::InferenceSource;
    use crate::domain::semantic::{ArtifactSemanticProfile, SemanticTag};
    use crate::ports::memory_canvas_repository::CanvasDraft;

    fn seed_canvas(path: &Path, name: &str) -> String {
        let repo = SqliteMemoryCanvasRepository::open(path).unwrap();
        let canvas = repo
            .create_canvas(CanvasDraft {
                workspace_id: "default".to_owned(),
                name: name.to_owned(),
                description: None,
            })
            .unwrap();
        canvas.id
    }

    fn seed_note(path: &Path, canvas_id: &str, text: &str, x: f64, y: f64) -> MemoryNode {
        let repo = SqliteMemoryCanvasRepository::open(path).unwrap();
        repo.add_node(NodeDraft {
            canvas_id: canvas_id.to_owned(),
            node_type: "note".to_owned(),
            position_x: x,
            position_y: y,
            width: None,
            height: None,
            payload_json: serde_json::json!({"text": text}).to_string(),
            summary: Some(truncate_chars(text, 50)),
            asset_id: None,
        })
        .unwrap()
    }

    fn seed_image(
        path: &Path,
        canvas_id: &str,
        payload_json: &str,
        asset_id: Option<&str>,
        x: f64,
    ) -> MemoryNode {
        let repo = SqliteMemoryCanvasRepository::open(path).unwrap();
        repo.add_node(NodeDraft {
            canvas_id: canvas_id.to_owned(),
            node_type: "image".to_owned(),
            position_x: x,
            position_y: 0.0,
            width: None,
            height: None,
            payload_json: payload_json.to_owned(),
            summary: None,
            asset_id: asset_id.map(str::to_owned),
        })
        .unwrap()
    }

    /// memory_nodes.asset_id 外键指向 asset_manifest，测试先落一条资产记录。
    fn seed_asset(path: &Path, asset_id: &str) {
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute(
            "INSERT INTO asset_manifest (id, storage_namespace, asset_kind, display_name, relative_path, size_bytes, mime_type, integrity_status, metadata_json, revision, created_at, updated_at) VALUES (?1, 'workspace', 'image', '测试图片', 'managed-files/test.png', 10, 'image/png', 'unverified', '{}', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            rusqlite::params![asset_id],
        )
        .unwrap();
    }

    fn seed_profile(path: &Path, asset_id: &str, caption: &str, tags: &[&str]) {
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS artifact_semantic_profiles (
                artifact_id TEXT PRIMARY KEY,
                caption TEXT,
                ocr_text TEXT,
                tags_json TEXT NOT NULL DEFAULT '[]',
                entities_json TEXT NOT NULL DEFAULT '[]',
                embedding_id TEXT,
                analyzer TEXT NOT NULL,
                analyzer_version TEXT NOT NULL,
                analyzed_at TEXT NOT NULL,
                description_short TEXT,
                description_detailed TEXT,
                objects_json TEXT NOT NULL DEFAULT '[]',
                scene_json TEXT NOT NULL DEFAULT '[]',
                actions_json TEXT NOT NULL DEFAULT '[]',
                concepts_json TEXT NOT NULL DEFAULT '[]',
                relations_json TEXT NOT NULL DEFAULT '[]',
                analysis_job_id TEXT
            );",
        )
        .unwrap();
        let repo = SqliteSemanticRepository::open(conn);
        let profile = ArtifactSemanticProfile {
            artifact_id: asset_id.to_owned(),
            caption: Some(caption.to_owned()),
            ocr_text: None,
            tags: tags
                .iter()
                .map(|t| SemanticTag {
                    name: (*t).to_owned(),
                    confidence: 0.9,
                    source: InferenceSource::Heuristic,
                })
                .collect(),
            entities: Vec::new(),
            embedding_id: None,
            analyzer: "qwen-vl".to_owned(),
            analyzer_version: "1".to_owned(),
            analyzed_at: "2026-01-01T00:00:00Z".to_owned(),
            description_short: None,
            description_detailed: None,
            objects: Vec::new(),
            scene: Vec::new(),
            actions: Vec::new(),
            concepts: Vec::new(),
            relations: Vec::new(),
            analysis_job_id: None,
        };
        repo.upsert_profile(&profile).unwrap();
    }

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
        let canvas_id = seed_canvas(&db, "conv-test");
        seed_note(&db, &canvas_id, "用户喜欢极简风格的设计", 0.0, 0.0);
        seed_image(
            &db,
            &canvas_id,
            r#"{"description":"山景照片"}"#,
            None,
            100.0,
        );

        let ctx = load_working_memory_context(dir.path(), "test", Some("极简风格 设计"), 1)
            .expect("should have working memory");
        assert!(ctx.contains("极简风格"), "ctx: {ctx}");

        let hits = search_nodes(dir.path(), "test", "山景", 3);
        assert!(!hits.is_empty());
        assert!(hits[0].1.contains("山景"));
    }

    #[test]
    fn search_returns_node_ids_for_update_tool() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        let canvas_id = seed_canvas(&db, "conv-test");
        let note = seed_note(&db, &canvas_id, "二维码处理需求", 0.0, 0.0);

        let hits = search_nodes(dir.path(), "test", "二维码", 3);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, note.id);
    }

    #[test]
    fn working_memory_includes_vision_analysis_and_edges() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        let canvas_id = seed_canvas(&db, "conv-test");
        seed_asset(&db, "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
        let note = seed_note(&db, &canvas_id, "海报需求：赛博朋克城市", 0.0, 0.0);
        let image = seed_image(
            &db,
            &canvas_id,
            r#"{"imageUrl":"https://cdn/x.png"}"#,
            Some("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
            100.0,
        );
        seed_profile(
            &db,
            "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "赛博朋克城市夜景，霓虹灯",
            &["赛博朋克", "夜景"],
        );

        // 建立一条连线 note → image
        let repo = SqliteMemoryCanvasRepository::open(&db).unwrap();
        repo.add_edge(EdgeDraft {
            canvas_id: canvas_id.clone(),
            source_node_id: note.id.clone(),
            target_node_id: image.id.clone(),
            edge_type: "reference".to_owned(),
            label: Some("内容补充".to_owned()),
        })
        .unwrap();
        drop(repo);

        let ctx = load_working_memory_context(dir.path(), "test", None, 10).unwrap();
        assert!(ctx.contains("视觉分析"), "ctx: {ctx}");
        assert!(ctx.contains("赛博朋克"), "ctx: {ctx}");
        assert!(ctx.contains("画布连线关系"), "ctx: {ctx}");
        assert!(ctx.contains("内容补充"), "ctx: {ctx}");
    }

    #[test]
    fn retrieval_boosts_connected_neighbors() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        let canvas_id = seed_canvas(&db, "conv-test");
        // 命中节点（赛博朋克）+ 其连线邻居（购物清单，文本不匹配）+ 无关孤立节点（霓虹）
        let hit = seed_note(&db, &canvas_id, "海报需求：赛博朋克城市", 0.0, 0.0);
        let neighbor = seed_note(&db, &canvas_id, "购物清单：牛奶面包", 100.0, 0.0);
        let _unrelated = seed_note(&db, &canvas_id, "霓虹灯维修记录", 200.0, 0.0);
        let repo = SqliteMemoryCanvasRepository::open(&db).unwrap();
        repo.add_edge(EdgeDraft {
            canvas_id: canvas_id.clone(),
            source_node_id: hit.id.clone(),
            target_node_id: neighbor.id.clone(),
            edge_type: "reference".to_owned(),
            label: None,
        })
        .unwrap();
        drop(repo);

        let ctx = load_working_memory_context(dir.path(), "test", Some("赛博朋克"), 3).unwrap();
        assert!(ctx.contains("购物清单"), "邻居应经加权进入上下文：{ctx}");
        // 无关且未连线的节点不应挤进检索结果
        assert!(
            !ctx.contains("霓虹灯维修"),
            "未连线的无关节点不应入选：{ctx}"
        );
    }

    #[test]
    fn reference_context_targets_reference_node_and_neighbors() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        let canvas_id = seed_canvas(&db, "conv-test");
        let reference = "data:image/png;base64,QUJD";
        // 参考图节点（dataUrl 匹配）+ 其连线邻居 + 无关节点
        let anchor = seed_image(
            &db,
            &canvas_id,
            r#"{"imageUrl":"https://cdn/orig.png","description":"参考原图"}"#,
            None,
            0.0,
        );
        // 用原生 SQL 不行——payload 需含 dataUrl 匹配；改用 imageUrl 双匹配参考
        let note = seed_note(&db, &canvas_id, "风格约束：保持暖色调", 100.0, 0.0);
        seed_note(&db, &canvas_id, "完全无关的内容", 200.0, 0.0);
        let repo = SqliteMemoryCanvasRepository::open(&db).unwrap();
        repo.add_edge(EdgeDraft {
            canvas_id: canvas_id.clone(),
            source_node_id: anchor.id.clone(),
            target_node_id: note.id.clone(),
            edge_type: "reference".to_owned(),
            label: Some("风格约束".to_owned()),
        })
        .unwrap();
        drop(repo);

        // dataUrl 与 imageUrl 双匹配：任一字段等于参考图即命中
        let ctx = load_reference_context(dir.path(), "test", "https://cdn/orig.png")
            .expect("参考图节点应被定位");
        assert!(ctx.contains("参考图本体"), "ctx: {ctx}");
        assert!(ctx.contains("参考原图"), "ctx: {ctx}");
        assert!(ctx.contains("风格约束"), "邻居的约束应进入上下文：{ctx}");
        assert!(!ctx.contains("完全无关"), "无关节点不应出现：{ctx}");
        let _ = reference;
    }

    #[test]
    fn overview_prefers_recently_updated_nodes() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        let canvas_id = seed_canvas(&db, "conv-test");
        // 先创建旧节点，再创建新节点；概览容量只放 1 个时应选新的
        seed_note(&db, &canvas_id, "最早的旧节点", 0.0, 0.0);
        seed_note(&db, &canvas_id, "最新的新节点", 100.0, 0.0);

        let ctx = load_working_memory_context(dir.path(), "test", None, 1).unwrap();
        assert!(ctx.contains("最新的新节点"), "概览应优先最近节点：{ctx}");
    }

    #[test]
    fn attach_generation_task_creates_pending_with_reference_link() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        let canvas_id = seed_canvas(&db, "conv-test");
        seed_image(
            &db,
            &canvas_id,
            r#"{"imageUrl":"https://cdn/ref.png"}"#,
            None,
            0.0,
        );
        // 参考图匹配按 dataUrl/imageUrl：这里直接给 imageUrl 作为参考串
        let node_id = attach_generation_task(
            dir.path(),
            "test",
            "task-42",
            "赛博朋克海报",
            Some("https://cdn/ref.png"),
        )
        .expect("pending 节点应创建");

        let repo = SqliteMemoryCanvasRepository::open(&db).unwrap();
        let nodes = repo.list_nodes(&canvas_id).unwrap();
        let pending = nodes.iter().find(|n| n.id == node_id).unwrap();
        let payload: serde_json::Value = serde_json::from_str(&pending.payload_json).unwrap();
        assert_eq!(payload["status"], "pending");
        assert_eq!(payload["taskId"], "task-42");
        let reference_id = payload["referenceNodeId"].as_str().unwrap();

        // 参考连线已建
        let edges = repo.list_edges(&canvas_id).unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source_node_id, reference_id);
        assert_eq!(edges[0].target_node_id, node_id);

        // 幂等：同 taskId 再次 attach 返回同节点、不新增边
        let again =
            attach_generation_task(dir.path(), "test", "task-42", "赛博朋克海报", None).unwrap();
        assert_eq!(again, node_id);
        assert_eq!(repo.list_edges(&canvas_id).unwrap().len(), 1);
    }

    #[test]
    fn complete_generation_task_by_task_id_fills_pending_across_canvases() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        let canvas_id = seed_canvas(&db, "conv-test");
        let node_id =
            attach_generation_task(dir.path(), "test", "task-7", "跳舞的猫", None).unwrap();

        let completed =
            complete_generation_task_by_task_id(dir.path(), "task-7", "https://cdn/result.png")
                .expect("应定位到 pending 节点");
        assert_eq!(completed, node_id);

        let repo = SqliteMemoryCanvasRepository::open(&db).unwrap();
        let nodes = repo.list_nodes(&canvas_id).unwrap();
        assert_eq!(nodes.len(), 1, "应复用 pending 节点而非新建");
        let payload: serde_json::Value = serde_json::from_str(&nodes[0].payload_json).unwrap();
        assert_eq!(payload["imageUrl"], "https://cdn/result.png");
        assert_eq!(payload["status"], "succeeded");

        // 不存在的 taskId：静默 None
        assert!(
            complete_generation_task_by_task_id(dir.path(), "task-none", "https://x").is_none()
        );
    }

    #[test]
    fn retrieval_two_hop_boost_reaches_second_degree() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        let canvas_id = seed_canvas(&db, "conv-test");
        // A 命中查询；A—B、B—C 链条；C 文本与查询无关
        let a = seed_note(&db, &canvas_id, "海报需求：赛博朋克城市", 0.0, 0.0);
        let b = seed_note(&db, &canvas_id, "构图备忘：竖版三分", 100.0, 0.0);
        let c = seed_note(&db, &canvas_id, "配色清单：黑紫渐变", 200.0, 0.0);
        let repo = SqliteMemoryCanvasRepository::open(&db).unwrap();
        for (from, to) in [(&a, &b), (&b, &c)] {
            repo.add_edge(EdgeDraft {
                canvas_id: canvas_id.clone(),
                source_node_id: from.id.clone(),
                target_node_id: to.id.clone(),
                edge_type: "reference".to_owned(),
                label: None,
            })
            .unwrap();
        }
        drop(repo);

        let ctx = load_working_memory_context(dir.path(), "test", Some("赛博朋克"), 3).unwrap();
        assert!(ctx.contains("配色清单"), "二跳邻居应经衰减加权入选：{ctx}");
        assert!(ctx.contains("构图备忘"), "一跳邻居应入选：{ctx}");
    }

    #[test]
    fn agent_note_sedimentation_collect_and_mark() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        let canvas_id = seed_canvas(&db, "conv-test");
        // Agent 便签（待沉淀）+ 用户便签 + 占位便签 + 已同步的 Agent 便签
        let seed_agent_note = |payload: serde_json::Value, x: f64| {
            let summary = payload
                .get("text")
                .and_then(|v| v.as_str())
                .map(|t| truncate_chars(t, 50));
            let repo = SqliteMemoryCanvasRepository::open(&db).unwrap();
            repo.add_node(NodeDraft {
                canvas_id: canvas_id.clone(),
                node_type: "note".to_owned(),
                position_x: x,
                position_y: 0.0,
                width: None,
                height: None,
                payload_json: payload.to_string(),
                summary,
                asset_id: None,
            })
            .unwrap()
        };
        seed_agent_note(
            serde_json::json!({"text": "Agent 结论：用暗色调", "source": "agent"}),
            0.0,
        );
        seed_note(&db, &canvas_id, "用户手写的内容", 100.0, 0.0);
        seed_agent_note(
            serde_json::json!({"text": "双击编辑便签", "source": "agent"}),
            200.0,
        );
        seed_agent_note(
            serde_json::json!({"text": "已沉淀结论", "source": "agent", "syncedToMemory": true}),
            300.0,
        );

        let collected = collect_unsynced_agent_notes(dir.path(), "test");
        assert_eq!(
            collected.len(),
            1,
            "只应收集未同步的 Agent 便签：{collected:?}"
        );
        assert!(collected[0].1.contains("Agent 结论"));

        mark_agent_notes_synced(dir.path(), "test", &[collected[0].0.clone()]);
        assert!(
            collect_unsynced_agent_notes(dir.path(), "test").is_empty(),
            "标记后不应再收集"
        );

        // DB 里确实写入了 syncedToMemory 标记
        let repo = SqliteMemoryCanvasRepository::open(&db).unwrap();
        let nodes = repo.list_nodes(&canvas_id).unwrap();
        let marked = nodes.iter().find(|n| n.id == collected[0].0).unwrap();
        let payload: serde_json::Value = serde_json::from_str(&marked.payload_json).unwrap();
        assert_eq!(payload["syncedToMemory"], serde_json::json!(true));
    }

    /// 手动端到端验证：对真实工作区数据跑完整记忆检索管线。
    /// 运行：cargo test --lib manual_real_workspace_memory -- --ignored --nocapture
    #[test]
    #[ignore = "manual e2e against real workspace"]
    fn manual_real_workspace_memory() {
        let workspace = std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap())
            .join("AppData")
            .join("Local")
            .join("com.aigcstudio.desktop")
            .join("workspace");
        // 找有节点数据的画布对应的对话（跳过空画布）
        let connection =
            rusqlite::Connection::open(workspace.join(DB_FILE)).expect("open workspace db");
        let conversations: Vec<String> = {
            let mut statement = connection
                .prepare(
                    "SELECT c.name FROM memory_canvases c
                     WHERE (SELECT COUNT(*) FROM memory_nodes n WHERE n.canvas_id = c.id AND n.deleted_at IS NULL) > 0
                     ORDER BY c.updated_at DESC LIMIT 3",
                )
                .expect("prepare");
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .expect("query");
            rows.flatten()
                .map(|name| name.trim_start_matches("conv-").to_owned())
                .collect()
        };
        eprintln!("conversations with data: {:?}", conversations.len());
        for conversation_id in &conversations {
            eprintln!("=== conversation {conversation_id} ===");
            // 概览模式
            match load_working_memory_context(&workspace, conversation_id, None, 24) {
                Some(ctx) => eprintln!(
                    "[overview {} chars]
{}",
                    ctx.chars().count(),
                    truncate_chars(&ctx, 600)
                ),
                None => eprintln!("[overview: empty]"),
            }
            // 检索模式（真实查询语义）
            match load_working_memory_context(
                &workspace,
                conversation_id,
                Some("生成一张海报 参考图片风格"),
                6,
            ) {
                Some(ctx) => eprintln!(
                    "[retrieval {} chars]
{}",
                    ctx.chars().count(),
                    truncate_chars(&ctx, 600)
                ),
                None => eprintln!("[retrieval: no match]"),
            }
        }
    }

    /// 手动端到端：在真实工作区画布上执行 canvas_add_note 的后端路径
    /// （与 Agent 工具完全相同的代码路径），并验证入库与自动连线。
    /// 运行：cargo test --lib manual_canvas_add_note_real -- --ignored --nocapture
    #[test]
    #[ignore = "manual e2e against real workspace"]
    fn manual_canvas_add_note_real() {
        let workspace = std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap())
            .join("AppData")
            .join("Local")
            .join("com.aigcstudio.desktop")
            .join("workspace");
        // 最近创建的对话（其画布当前为空）
        let connection =
            rusqlite::Connection::open(workspace.join(DB_FILE)).expect("open workspace db");
        let conversation_id: String = connection
            .query_row(
                "SELECT id FROM agent_conversations ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("latest conversation");

        let node_id = add_canvas_note(
            &workspace,
            &conversation_id,
            200.0,
            200.0,
            "项目主线是暗色调科技感",
        )
        .expect("note creation");
        eprintln!("note created: conversation={conversation_id} node={node_id}");

        // 自动连线（与 execute_canvas_add_note 相同的后置动作）
        let connected = auto_connect_related(&workspace, &conversation_id, &node_id, 3, None);
        eprintln!("auto_connected: {connected}");

        // 入库验证
        let repo = SqliteMemoryCanvasRepository::open(&workspace.join(DB_FILE)).unwrap();
        let canvas_id = find_canvas_id(&repo, &conversation_id).unwrap();
        let nodes = repo.list_nodes(&canvas_id).unwrap();
        let note = nodes.iter().find(|n| n.id == node_id).expect("note in db");
        let payload: serde_json::Value = serde_json::from_str(&note.payload_json).unwrap();
        assert_eq!(payload["text"], "项目主线是暗色调科技感");
        assert_eq!(payload["source"], "agent");
        assert_eq!(note.node_type, "note");
        eprintln!(
            "verified in db: type={} summary={:?} canvas_nodes={}",
            note.node_type,
            note.summary,
            nodes.len()
        );
    }

    #[test]
    fn cleanup_orphan_canvases_removes_all_when_no_conversations_exist() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        // 生产命名约定：conv-{conversation_id}
        seed_canvas(&db, "conv-orphan-a");
        seed_canvas(&db, "conv-orphan-b");
        // 测试环境无 agent_conversations 行 → 所有画布均为孤儿

        let cleaned = cleanup_orphan_canvases(dir.path());
        assert_eq!(cleaned, 2);

        // 幂等：再次运行为 0
        assert_eq!(cleanup_orphan_canvases(dir.path()), 0);
        // 画布已被软删：find_canvas_id 不再可见
        assert!(find_canvas_id(
            &SqliteMemoryCanvasRepository::open(&db).unwrap(),
            "orphan-a"
        )
        .is_none());
    }

    #[test]
    fn context_is_capped_at_max_chars() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        let canvas_id = seed_canvas(&db, "conv-test");
        let long_text = "长".repeat(300);
        for i in 0..30 {
            seed_note(
                &db,
                &canvas_id,
                &format!("{long_text} #{i}"),
                i as f64 * 10.0,
                0.0,
            );
        }
        let ctx = load_working_memory_context(dir.path(), "test", None, 30).unwrap();
        assert!(ctx.chars().count() <= MAX_CONTEXT_CHARS + 40);
    }

    #[test]
    fn add_image_dedupes_by_url_and_task() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        seed_canvas(&db, "conv-test");

        let first = add_canvas_image(
            dir.path(),
            "test",
            "https://cdn/a.png",
            Some("生成的猫"),
            None,
            None,
            Some("task-1"),
        )
        .unwrap();
        let duplicate = add_canvas_image(
            dir.path(),
            "test",
            "https://cdn/a.png",
            None,
            Some(999.0),
            Some(999.0),
            Some("task-2"),
        )
        .unwrap();
        assert_eq!(first, duplicate);

        let repo = SqliteMemoryCanvasRepository::open(&db).unwrap();
        let canvas_id = find_canvas_id(&repo, "test").unwrap();
        assert_eq!(repo.list_nodes(&canvas_id).unwrap().len(), 1);
    }

    #[test]
    fn update_node_changes_text_and_color() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        let canvas_id = seed_canvas(&db, "conv-test");
        let note = seed_note(&db, &canvas_id, "原始内容", 0.0, 0.0);

        let updated = update_canvas_node(
            dir.path(),
            "test",
            &note.id,
            Some("更新后的内容"),
            Some("#fbbf24"),
        )
        .unwrap();
        assert_eq!(updated, note.id);

        let repo = SqliteMemoryCanvasRepository::open(&db).unwrap();
        let node = repo
            .list_nodes(&canvas_id)
            .unwrap()
            .into_iter()
            .find(|n| n.id == note.id)
            .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&node.payload_json).unwrap();
        assert_eq!(payload["text"], "更新后的内容");
        assert_eq!(payload["color"], "#fbbf24");
        assert_eq!(node.summary.as_deref(), Some("更新后的内容"));
    }

    #[test]
    fn auto_layout_groups_by_type() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        let canvas_id = seed_canvas(&db, "conv-test");
        seed_note(&db, &canvas_id, "便签一", 900.0, 900.0);
        seed_note(&db, &canvas_id, "便签二", 1200.0, 300.0);
        seed_image(
            &db,
            &canvas_id,
            r#"{"imageUrl":"https://cdn/x.png"}"#,
            None,
            500.0,
        );

        let moved = auto_layout_canvas(dir.path(), "test");
        assert_eq!(moved, 3);

        let repo = SqliteMemoryCanvasRepository::open(&db).unwrap();
        let nodes = repo.list_nodes(&canvas_id).unwrap();
        let image = nodes.iter().find(|n| n.node_type == "image").unwrap();
        assert_eq!(image.position_x, 60.0); // image 排在第一列
        let notes: Vec<_> = nodes
            .iter()
            .filter(|n| n.node_type == "note")
            .map(|n| (n.position_x, n.position_y))
            .collect();
        assert!(notes.contains(&(340.0, 60.0)));
        assert!(notes.contains(&(340.0, 320.0)));

        // 再次整理：无位移
        assert_eq!(auto_layout_canvas(dir.path(), "test"), 0);
    }

    #[test]
    fn export_omits_data_url() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        let canvas_id = seed_canvas(&db, "conv-test");
        seed_image(
            &db,
            &canvas_id,
            r#"{"dataUrl":"data:image/png;base64,QUJD","text":"截图"}"#,
            None,
            0.0,
        );

        let exported = export_canvas(dir.path(), "test").unwrap();
        let text = exported.to_string();
        assert!(text.contains("已省略"));
        assert!(!text.contains("QUJD"));
        assert_eq!(exported["nodeCount"], serde_json::json!(1));
    }

    #[test]
    fn auto_connect_links_to_image_via_profile() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        let canvas_id = seed_canvas(&db, "conv-test");
        seed_asset(&db, "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
        let note = seed_note(&db, &canvas_id, "海报需求：赛博朋克城市", 0.0, 0.0);
        let image = seed_image(
            &db,
            &canvas_id,
            r#"{"imageUrl":"https://cdn/x.png"}"#,
            Some("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
            100.0,
        );
        seed_profile(
            &db,
            "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "赛博朋克城市夜景",
            &[],
        );

        let connected = auto_connect_related(dir.path(), "test", &note.id, 3, None);
        assert_eq!(connected, 1, "图片节点应经视觉分析被关联上");

        let repo = SqliteMemoryCanvasRepository::open(&db).unwrap();
        let edges = repo.list_edges(&canvas_id).unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source_node_id, note.id);
        assert_eq!(edges[0].target_node_id, image.id);
        assert_eq!(edges[0].label.as_deref(), Some("语义相关"));
    }

    #[test]
    fn auto_connect_label_derives_from_user_message() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        let canvas_id = seed_canvas(&db, "conv-test");
        let note = seed_note(&db, &canvas_id, "二维码处理需求", 0.0, 0.0);
        seed_note(&db, &canvas_id, "去除二维码的方法", 100.0, 0.0);

        let connected =
            auto_connect_related(dir.path(), "test", &note.id, 3, Some("请帮我去除二维码"));
        assert_eq!(connected, 1);

        let repo = SqliteMemoryCanvasRepository::open(&db).unwrap();
        let edges = repo.list_edges(&canvas_id).unwrap();
        assert_eq!(edges[0].label.as_deref(), Some("内容补充"));
    }

    #[test]
    fn generation_image_node_reuses_pending_placeholder_and_links_reference() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        let canvas_id = seed_canvas(&db, "conv-test");
        // 前端面板打开时创建的 pending 占位节点 + 参考图（用户上传）
        seed_image(
            &db,
            &canvas_id,
            r#"{"prompt":"跳舞的猫","status":"pending"}"#,
            None,
            100.0,
        );
        seed_image(
            &db,
            &canvas_id,
            r#"{"dataUrl":"data:image/png;base64,REFUQQ==","source":"user-upload"}"#,
            None,
            0.0,
        );

        let (node_id, linked) = ensure_generation_image_node(
            dir.path(),
            "test",
            "task-1",
            "https://cdn/result.png",
            "跳舞的猫",
            Some("data:image/png;base64,REFUQQ=="),
        )
        .unwrap();

        assert!(linked, "应建立到参考图的连线");
        let repo = SqliteMemoryCanvasRepository::open(&db).unwrap();
        let nodes = repo.list_nodes(&canvas_id).unwrap();
        assert_eq!(nodes.len(), 2, "占位节点应被复用而不是新建");
        let target = nodes.iter().find(|n| n.id == node_id).unwrap();
        let payload: serde_json::Value = serde_json::from_str(&target.payload_json).unwrap();
        assert_eq!(payload["imageUrl"], "https://cdn/result.png");
        assert_eq!(payload["status"], "succeeded");

        let edges = repo.list_edges(&canvas_id).unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].label.as_deref(), Some("风格参考"));
    }

    #[test]
    fn generation_image_node_creates_when_panel_closed_and_dedupes() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        seed_canvas(&db, "conv-test");

        let (first, linked) = ensure_generation_image_node(
            dir.path(),
            "test",
            "task-1",
            "https://cdn/result.png",
            "跳舞的猫",
            None,
        )
        .unwrap();
        assert!(!linked);
        let (second, _) = ensure_generation_image_node(
            dir.path(),
            "test",
            "task-1",
            "https://cdn/result.png",
            "跳舞的猫",
            None,
        )
        .unwrap();
        assert_eq!(first, second, "同一 taskId 不应重复建节点");

        let repo = SqliteMemoryCanvasRepository::open(&db).unwrap();
        let canvas_id = find_canvas_id(&repo, "test").unwrap();
        assert_eq!(repo.list_nodes(&canvas_id).unwrap().len(), 1);
    }
}
