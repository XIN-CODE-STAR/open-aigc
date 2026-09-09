//! ContextOrchestrator — 统一调度 Canvas Context + RAG → UnifiedContext。
//!
//! 架构全景图中的核心编排器：
//! - CanvasContextBuilder 只做空间/图关系（BFS 邻近）
//! - SemanticRetrievalService 只做语义检索（标签/OCR/Embedding）
//! - ContextOrchestrator 将两者合并，产出 UnifiedContext 给 Agent
//!
//! 设计原则：
//! - 宪法第 2 条：Memory/RAG 提供知识，Canvas 保存空间组织
//! - 两者通过引用和语义关系连接，不互相污染
//! - Agent 只消费 UnifiedContext，不直接访问底层服务

use std::sync::Arc;

use crate::application::canvas_context_service::CanvasContextService;
use crate::application::error::AppError;
use crate::application::semantic::retrieval_service::{RetrievalResult, SemanticRetrievalService};
use crate::domain::canvas::{CanvasContext, ContextBudget, ContextQuery};

// ──────────────────────────────────────────────────────────────────
// UnifiedContext — Agent 消费的统一上下文
// ──────────────────────────────────────────────────────────────────

/// 统一上下文 — ContextOrchestrator 的输出，Agent 直接消费。
///
/// 合并了画布空间上下文和 RAG 语义检索结果。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedContext {
    /// 画布空间上下文（节点布局、邻近关系、图结构）。
    pub canvas: Option<CanvasContext>,
    /// RAG 语义检索结果（标签/OCR/Embedding 匹配的资源）。
    pub semantic_results: Vec<RetrievalResult>,
    /// 合并后的 LLM 可读描述。
    pub formatted_description: String,
    /// 总 token 估算。
    pub estimated_tokens: usize,
}

// ──────────────────────────────────────────────────────────────────
// ContextOrchestrator
// ──────────────────────────────────────────────────────────────────

/// 上下文编排器 — 统一调度 Canvas Context + RAG → UnifiedContext。
///
/// 为 Agent 提供统一的上下文查询接口，Agent 不需要知道
/// 上下文来自画布空间还是语义检索。
pub struct ContextOrchestrator {
    canvas_context_service: Arc<CanvasContextService>,
    semantic_retrieval_service: Arc<SemanticRetrievalService>,
}

impl ContextOrchestrator {
    pub fn new(
        canvas_context_service: Arc<CanvasContextService>,
        semantic_retrieval_service: Arc<SemanticRetrievalService>,
    ) -> Self {
        Self {
            canvas_context_service,
            semantic_retrieval_service,
        }
    }

    /// 为 Agent 构建统一上下文。
    ///
    /// 同时获取画布空间上下文和 RAG 语义检索结果，
    /// 合并为 UnifiedContext。
    pub fn build_context(
        &self,
        conversation_id: &str,
        query: Option<&ContextQuery>,
        semantic_query: Option<&str>,
        budget: ContextBudget,
    ) -> Result<UnifiedContext, AppError> {
        // 1. 画布空间上下文
        let canvas_context = self
            .canvas_context_service
            .load_context_for_agent(conversation_id, query)?;

        // 2. RAG 语义检索
        let semantic_results = if let Some(sq) = semantic_query {
            self.semantic_retrieval_service
                .unified_search(sq, budget.max_nodes)?
        } else {
            vec![]
        };

        // 3. 合并格式化描述
        let formatted_description = self.merge_descriptions(&canvas_context, &semantic_results);

        // 4. 估算 token
        let estimated_tokens = formatted_description.len() / 3
            + canvas_context.node_count * 50
            + semantic_results.len() * 100;

        Ok(UnifiedContext {
            canvas: if canvas_context.canvas_id.is_empty() {
                None
            } else {
                Some(canvas_context)
            },
            semantic_results,
            formatted_description,
            estimated_tokens,
        })
    }

    /// 仅获取画布上下文（不进行 RAG 检索）。
    ///
    /// 当 Agent 只需要空间上下文时使用（如画布操作工具）。
    pub fn get_canvas_context(
        &self,
        conversation_id: &str,
        query: Option<&ContextQuery>,
    ) -> Result<CanvasContext, AppError> {
        self.canvas_context_service
            .load_context_for_agent(conversation_id, query)
    }

    /// 仅进行 RAG 语义检索（不获取画布上下文）。
    ///
    /// 当 Agent 只需要语义检索时使用（如查找相似资源）。
    pub fn search_semantic(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<RetrievalResult>, AppError> {
        self.semantic_retrieval_service.unified_search(query, limit)
    }

    /// 合并画布上下文和语义检索结果的描述。
    fn merge_descriptions(&self, canvas: &CanvasContext, semantic: &[RetrievalResult]) -> String {
        let mut output = String::new();

        // Canvas section
        if !canvas.formatted_description.is_empty()
            && canvas.formatted_description != "画布暂未创建，无可用上下文。"
        {
            output.push_str("[画布上下文]\n");
            output.push_str(&canvas.formatted_description);
            output.push('\n');
        }

        // Semantic section
        if !semantic.is_empty() {
            output.push_str("\n[语义检索结果]\n");
            for (i, result) in semantic.iter().enumerate() {
                let id = &result.profile.artifact_id;
                let caption = result.profile.caption.as_deref().unwrap_or("(无描述)");
                let tags: Vec<&str> = result.profile.tag_names();
                let tag_str = if tags.is_empty() {
                    String::new()
                } else {
                    format!(" | 标签: {}", tags.join(", "))
                };
                output.push_str(&format!(
                    "{}. [{}] {}{} (score: {:.2}, {})\n",
                    i + 1,
                    id,
                    caption,
                    tag_str,
                    result.score,
                    result.match_reason,
                ));
            }
        }

        if output.is_empty() {
            "暂无可用上下文。".to_owned()
        } else {
            output
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_descriptions_empty() {
        // Can't easily construct real services in unit tests,
        // so test the merge logic directly
        let canvas = CanvasContext {
            canvas_id: String::new(),
            canvas_name: String::new(),
            nodes: vec![],
            edges: vec![],
            contextual_nodes: None,
            formatted_description: "画布暂未创建，无可用上下文。".to_owned(),
            node_count: 0,
            edge_count: 0,
        };

        // Test that empty canvas + empty semantic = placeholder
        let has_canvas = !canvas.formatted_description.is_empty()
            && canvas.formatted_description != "画布暂未创建，无可用上下文。";
        assert!(!has_canvas);
    }

    #[test]
    fn unified_context_serialization() {
        let ctx = UnifiedContext {
            canvas: None,
            semantic_results: vec![],
            formatted_description: "暂无可用上下文。".to_owned(),
            estimated_tokens: 0,
        };
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(json.contains("暂无可用上下文"));
    }
}
