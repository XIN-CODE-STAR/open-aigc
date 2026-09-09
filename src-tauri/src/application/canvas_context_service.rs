//! CanvasContextService — Agent 工具的画布上下文服务。
//!
//! 替代 builtin_tools.rs 中直接访问 SQLite 的 load_canvas_context。
//! Agent 工具通过此服务获取画布上下文，而不是直接操作数据库。
//!
//! 设计原则：
//! - 应用层服务，编排 CanvasRepository + CanvasContextBuilder
//! - 返回结构化 CanvasContext（不是原始 SQL 结果）
//! - 通过 conversation_ref 直接 ID 关联查找画布（不使用名称约定）

use std::sync::{Arc, Mutex};

use crate::application::canvas_context_builder::CanvasContextBuilder;
use crate::application::error::AppError;
use crate::domain::canvas::{
    CanvasContext, CanvasNode, ContextBudget, ContextQuery, ContextualNode, PromptContext,
};
use crate::ports::canvas_repository::CanvasRepository;

// ──────────────────────────────────────────────────────────────────
// CanvasContextService
// ──────────────────────────────────────────────────────────────────

/// 画布上下文服务。
///
/// 为 Agent 工具和 PromptCompiler 提供画布上下文查询。
/// 不直接暴露 CanvasRepository，而是提供面向使用场景的查询接口。
pub struct CanvasContextService {
    canvas_repo: Arc<Mutex<dyn CanvasRepository>>,
}

impl CanvasContextService {
    pub fn new(canvas_repo: Arc<Mutex<dyn CanvasRepository>>) -> Self {
        Self { canvas_repo }
    }

    /// 为 Agent 加载画布上下文 — 替代 builtin_tools 中的 load_canvas_context。
    ///
    /// 通过 conversation_ref 直接 ID 关联查找画布（不使用名称约定），
    /// 返回结构化的 CanvasContext，包含节点列表、边列表、格式化描述。
    pub fn load_context_for_agent(
        &self,
        conversation_id: &str,
        query: Option<&ContextQuery>,
    ) -> Result<CanvasContext, AppError> {
        let repo = self
            .canvas_repo
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;

        // 直接通过 conversation_ref 查找画布（不使用 conv-{id} 名称约定）
        let canvas = repo.find_canvas_by_conversation_ref(conversation_id)?;

        let canvas = match canvas {
            Some(c) => c,
            None => {
                return Ok(CanvasContext {
                    canvas_id: String::new(),
                    canvas_name: String::new(),
                    nodes: vec![],
                    edges: vec![],
                    contextual_nodes: None,
                    formatted_description: "画布暂未创建，无可用上下文。".to_owned(),
                    node_count: 0,
                    edge_count: 0,
                });
            }
        };

        let nodes = repo.list_nodes(&canvas.id)?;
        let edges = repo.list_edges(&canvas.id)?;

        // Build contextual nodes if query specified
        let contextual_nodes = query.map(|q| {
            let budget = ContextBudget::default();
            CanvasContextBuilder::select_relevant_nodes(&nodes, &edges, q, &budget)
        });

        let formatted_description = match &contextual_nodes {
            Some(cn) => CanvasContextBuilder::format_for_llm(cn),
            None => {
                // Full canvas description (backward compatible)
                Self::format_full_canvas(&canvas.name, &nodes, &edges)
            }
        };

        let node_count = nodes.len();
        let edge_count = edges.len();

        Ok(CanvasContext {
            canvas_id: canvas.id,
            canvas_name: canvas.name,
            nodes,
            edges,
            contextual_nodes,
            formatted_description,
            node_count,
            edge_count,
        })
    }

    /// 为 PromptCompiler 获取精简的提示词上下文。
    ///
    /// 以 shot_id 为中心，选择相关的节点（角色引用、场景描述、前后镜头）。
    pub fn get_context_for_prompt(
        &self,
        canvas_id: &str,
        shot_id: &str,
        budget: ContextBudget,
    ) -> Result<PromptContext, AppError> {
        let repo = self
            .canvas_repo
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;

        let nodes = repo.list_nodes(canvas_id)?;
        let edges = repo.list_edges(canvas_id)?;

        // Find the shot node by shot_ref
        let shot_node = nodes
            .iter()
            .find(|n| n.refs.shot_id.as_deref() == Some(shot_id));

        let contextual_nodes = if let Some(shot) = shot_node {
            let query = ContextQuery {
                center_node_id: Some(shot.id.clone()),
                include_relations: true,
                max_depth: Some(2),
                ..Default::default()
            };
            CanvasContextBuilder::select_relevant_nodes(&nodes, &edges, &query, &budget)
        } else {
            vec![]
        };

        // Extract character descriptions from character nodes
        let character_descriptions: Vec<String> = contextual_nodes
            .iter()
            .filter(|cn| cn.node.kind == crate::domain::canvas::CanvasNodeKind::Character)
            .filter_map(|cn| {
                cn.node
                    .description
                    .clone()
                    .or_else(|| cn.node.summary.clone())
            })
            .collect();

        // Extract related artifact prompts
        let related_artifact_prompts: Vec<String> = contextual_nodes
            .iter()
            .filter(|cn| cn.node.kind == crate::domain::canvas::CanvasNodeKind::Image)
            .filter_map(|cn| cn.node.prompt.clone())
            .collect();

        // Find scene description
        let scene_description = contextual_nodes
            .iter()
            .find(|cn| cn.node.kind == crate::domain::canvas::CanvasNodeKind::Scene)
            .and_then(|cn| {
                cn.node
                    .description
                    .clone()
                    .or_else(|| cn.node.summary.clone())
            });

        let estimated_tokens = contextual_nodes.len() * 100; // rough estimate

        Ok(PromptContext {
            contextual_nodes,
            related_artifact_prompts,
            character_descriptions,
            scene_description,
            estimated_tokens,
        })
    }

    /// 格式化完整画布描述（当没有查询过滤时的默认行为）。
    fn format_full_canvas(
        canvas_name: &str,
        nodes: &[CanvasNode],
        edges: &[crate::domain::canvas::CanvasEdge],
    ) -> String {
        use std::collections::HashMap;

        let mut output = String::new();
        output.push_str(&format!("[画布: {canvas_name}]\n"));
        output.push_str(&format!(
            "节点数: {}, 连线数: {}\n\n",
            nodes.len(),
            edges.len()
        ));

        // Type distribution
        let mut type_counts: HashMap<&str, usize> = HashMap::new();
        for n in nodes {
            *type_counts.entry(n.kind.as_str()).or_insert(0) += 1;
        }
        let dist: Vec<String> = type_counts
            .iter()
            .map(|(t, c)| format!("{t}*{c}"))
            .collect();
        output.push_str(&format!("类型分布: {}\n\n", dist.join(", ")));

        // Node list
        output.push_str("[节点列表]\n");
        for n in nodes {
            let kind = n.kind.as_str();
            let summary = n.summary.as_deref().unwrap_or("(无摘要)");
            output.push_str(&format!(
                "• [{kind}] {summary} ({}, {})",
                n.position.x as i64, n.position.y as i64,
            ));
            if let Some(status) = n.status {
                output.push_str(&format!(" [{}]", status.as_str()));
            }
            if let Some(ref prompt) = n.prompt {
                let truncated = if prompt.chars().count() > 80 {
                    let cut: String = prompt.chars().take(80).collect();
                    format!("{cut}…")
                } else {
                    prompt.clone()
                };
                output.push_str(&format!(" | {truncated}"));
            }
            output.push('\n');
        }

        // Edge list
        if !edges.is_empty() {
            output.push_str("\n[连接关系]\n");
            for e in edges {
                let src_summary = nodes
                    .iter()
                    .find(|n| n.id == e.source_node_id)
                    .and_then(|n| n.summary.as_deref())
                    .unwrap_or(&e.source_node_id[..8.min(e.source_node_id.len())]);
                let tgt_summary = nodes
                    .iter()
                    .find(|n| n.id == e.target_node_id)
                    .and_then(|n| n.summary.as_deref())
                    .unwrap_or(&e.target_node_id[..8.min(e.target_node_id.len())]);
                let kind = e.kind.as_str();
                output.push_str(&format!("• {src_summary} → {tgt_summary} ({kind})\n"));
            }
        }

        output
    }

    /// 更新画布节点的部分字段。
    pub fn update_node(
        &self,
        node_id: &str,
        patch: crate::domain::canvas::NodePatch,
    ) -> Result<(), AppError> {
        let repo = self
            .canvas_repo
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.update_node(node_id, patch)?;
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_full_canvas_empty() {
        let text = CanvasContextService::format_full_canvas("test-canvas", &[], &[]);
        assert!(text.contains("test-canvas"));
        assert!(text.contains("节点数: 0"));
    }
}
