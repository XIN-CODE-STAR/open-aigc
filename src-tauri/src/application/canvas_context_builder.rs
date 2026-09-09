//! CanvasContextBuilder — 语义空间关系计算 + 相关上下文选择。
//!
//! 解决的问题：不要每次把整个画布全部塞给 LLM。
//! 取而代之的是选择相关的节点子集，并计算语义空间关系（left_of, connected_to 等）。
//!
//! 设计原则：
//! - 纯计算（无 I/O、无数据库访问）
//! - 输出 ContextualNode[] 供 CanvasContextService 和 PromptCompiler 使用
//! - 支持多种查询模式：按类型过滤、按引用过滤、按邻近搜索

use std::collections::{HashMap, HashSet, VecDeque};

use crate::domain::canvas::{
    CanvasEdge, CanvasEdgeKind, CanvasNode, CanvasNodeKind, CanvasPosition, ContextBudget,
    ContextQuery, ContextualNode, DistanceCategory, RefFilter, SemanticRelation,
};

// ──────────────────────────────────────────────────────────────────
// CanvasContextBuilder
// ──────────────────────────────────────────────────────────────────

pub struct CanvasContextBuilder;

impl CanvasContextBuilder {
    /// 计算所有节点对之间的语义空间关系。
    ///
    /// 返回 (node_a_id, node_b_id, relation) 三元组列表。
    /// 不计算所有 N² 对，只计算有意义的关系：
    /// 1. 通过边直接连接的节点
    /// 2. 水平/垂直方向上距离较近的节点
    /// 3. 同一场景组中的节点
    pub fn build_semantic_relations(
        nodes: &[CanvasNode],
        edges: &[CanvasEdge],
    ) -> Vec<(String, String, SemanticRelation)> {
        let mut relations = Vec::new();

        // 1. Edge-based relations
        let node_map: HashMap<&str, &CanvasNode> =
            nodes.iter().map(|n| (n.id.as_str(), n)).collect();

        for edge in edges {
            let edge_kind = edge.kind;
            relations.push((
                edge.source_node_id.clone(),
                edge.target_node_id.clone(),
                SemanticRelation::ConnectedTo { edge_kind },
            ));

            // Sequence edges also imply temporal ordering
            if edge.kind == CanvasEdgeKind::Sequence {
                relations.push((
                    edge.source_node_id.clone(),
                    edge.target_node_id.clone(),
                    SemanticRelation::TemporalBefore,
                ));
                relations.push((
                    edge.target_node_id.clone(),
                    edge.source_node_id.clone(),
                    SemanticRelation::TemporalAfter,
                ));
            }

            // Composition edges also imply "in same scene"
            if edge.kind == CanvasEdgeKind::Composition {
                // The target (parent) contains the source (child)
                // All children of the same parent are "in same scene"
                let parent_id = &edge.target_node_id;
                let children: Vec<&str> = edges
                    .iter()
                    .filter(|e| {
                        e.kind == CanvasEdgeKind::Composition && e.target_node_id == *parent_id
                    })
                    .map(|e| e.source_node_id.as_str())
                    .collect();

                for (i, a) in children.iter().enumerate() {
                    for b in children.iter().skip(i + 1) {
                        relations.push((
                            a.to_string(),
                            b.to_string(),
                            SemanticRelation::InSameScene,
                        ));
                    }
                }
            }
        }

        // 2. Spatial proximity relations (only for nodes that are close)
        for (i, a) in nodes.iter().enumerate() {
            for b in nodes.iter().skip(i + 1) {
                let dx = b.position.x - a.position.x;
                let dy = b.position.y - a.position.y;
                let dist = (dx * dx + dy * dy).sqrt();

                // Only compute directional relations for nearby nodes (< 400px)
                if dist < 400.0 {
                    let abs_dx = dx.abs();
                    let abs_dy = dy.abs();

                    // Primarily horizontal
                    if abs_dx > abs_dy * 1.5 {
                        if dx > 0.0 {
                            relations.push((a.id.clone(), b.id.clone(), SemanticRelation::RightOf));
                            relations.push((b.id.clone(), a.id.clone(), SemanticRelation::LeftOf));
                        } else {
                            relations.push((a.id.clone(), b.id.clone(), SemanticRelation::LeftOf));
                            relations.push((b.id.clone(), a.id.clone(), SemanticRelation::RightOf));
                        }
                    }
                    // Primarily vertical
                    else if abs_dy > abs_dx * 1.5 {
                        if dy > 0.0 {
                            relations.push((a.id.clone(), b.id.clone(), SemanticRelation::Below));
                            relations.push((b.id.clone(), a.id.clone(), SemanticRelation::Above));
                        } else {
                            relations.push((a.id.clone(), b.id.clone(), SemanticRelation::Above));
                            relations.push((b.id.clone(), a.id.clone(), SemanticRelation::Below));
                        }
                    }

                    // Nearest classification
                    let category = if dist < 100.0 {
                        DistanceCategory::Adjacent
                    } else if dist < 300.0 {
                        DistanceCategory::Near
                    } else {
                        DistanceCategory::Far
                    };

                    relations.push((
                        a.id.clone(),
                        b.id.clone(),
                        SemanticRelation::Nearest {
                            distance_category: category,
                        },
                    ));
                }
            }
        }

        relations
    }

    /// 根据查询条件选择相关的节点子集。
    ///
    /// 按以下优先级选择：
    /// 1. 如果有 center_node_id，从该节点出发做 BFS 邻近搜索
    /// 2. 如果有 kinds 过滤，只保留匹配的类型
    /// 3. 如果有 ref_filter，按引用字段过滤
    /// 4. 按 relevance_score 排序，截取 budget.max_nodes
    pub fn select_relevant_nodes(
        nodes: &[CanvasNode],
        edges: &[CanvasEdge],
        query: &ContextQuery,
        budget: &ContextBudget,
    ) -> Vec<ContextualNode> {
        let relations = if query.include_relations {
            Self::build_semantic_relations(nodes, edges)
        } else {
            vec![]
        };

        // Build adjacency map for BFS
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
        for edge in edges {
            adjacency
                .entry(edge.source_node_id.clone())
                .or_default()
                .push(edge.target_node_id.clone());
            adjacency
                .entry(edge.target_node_id.clone())
                .or_default()
                .push(edge.source_node_id.clone());
        }

        // Step 1: Filter by kinds if specified
        let filtered: Vec<&CanvasNode> = if let Some(ref kinds) = query.kinds {
            nodes.iter().filter(|n| kinds.contains(&n.kind)).collect()
        } else {
            nodes.iter().collect()
        };

        // Step 2: Filter by ref if specified
        let ref_filtered: Vec<&CanvasNode> = if let Some(ref rf) = query.ref_filter {
            filtered
                .into_iter()
                .filter(|n| Self::matches_ref_filter(n, rf))
                .collect()
        } else {
            filtered
        };

        // Step 3: If center_node_id specified, do BFS and score by distance
        let scored: Vec<(&CanvasNode, f64)> = if let Some(ref center_id) = query.center_node_id {
            let max_depth = query.max_depth.unwrap_or(3);
            let distances = Self::bfs_distances(center_id, &adjacency, max_depth);

            ref_filtered
                .into_iter()
                .filter_map(|n| {
                    if n.id == *center_id {
                        Some((n, 1.0))
                    } else if let Some(&dist) = distances.get(&n.id) {
                        let score = 1.0 / (1.0 + dist as f64);
                        Some((n, score))
                    } else {
                        None // Not reachable within max_depth
                    }
                })
                .collect()
        } else {
            // No center — score by recency (use created_at as proxy)
            // For simplicity, give all equal score
            ref_filtered.into_iter().map(|n| (n, 0.5)).collect()
        };

        // Step 4: Sort by score, truncate to budget
        let mut sorted = scored;
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        sorted.truncate(budget.max_nodes);

        // Step 5: Build ContextualNode with relations
        let selected_ids: HashSet<&str> = sorted.iter().map(|(n, _)| n.id.as_str()).collect();

        sorted
            .into_iter()
            .map(|(node, score)| {
                let node_relations: Vec<(String, SemanticRelation)> = relations
                    .iter()
                    .filter(|(a, b, _)| {
                        (a == &node.id && selected_ids.contains(b.as_str()))
                            || (b == &node.id && selected_ids.contains(a.as_str()))
                    })
                    .map(|(a, b, r)| {
                        let other = if a == &node.id { b.clone() } else { a.clone() };
                        (other, r.clone())
                    })
                    .collect();

                ContextualNode {
                    node: node.clone(),
                    relations: node_relations,
                    relevance_score: score,
                }
            })
            .collect()
    }

    /// 格式化上下文节点为 LLM 可读的文本描述。
    pub fn format_for_llm(contextual_nodes: &[ContextualNode]) -> String {
        if contextual_nodes.is_empty() {
            return "画布为空，暂无节点。".to_owned();
        }

        let mut output = String::new();
        output.push_str(&format!(
            "[画布上下文 — {} 个相关节点]\n\n",
            contextual_nodes.len()
        ));

        for cn in contextual_nodes {
            let node = &cn.node;
            let kind_label = Self::kind_label(node.kind);
            output.push_str(&format!(
                "• [{}] {} ({}, {})",
                kind_label,
                node.summary.as_deref().unwrap_or("(无摘要)"),
                node.position.x as i64,
                node.position.y as i64,
            ));

            if let Some(status) = node.status {
                output.push_str(&format!(" [{}]", status.as_str()));
            }

            if let Some(ref prompt) = node.prompt {
                let truncated = if prompt.chars().count() > 100 {
                    let cut: String = prompt.chars().take(100).collect();
                    format!("{cut}…")
                } else {
                    prompt.clone()
                };
                output.push_str(&format!("\n  prompt: {truncated}"));
            }

            // Show semantic relations
            if !cn.relations.is_empty() {
                output.push_str("\n  关系: ");
                let rel_strs: Vec<String> = cn
                    .relations
                    .iter()
                    .map(|(other_id, rel)| {
                        format!(
                            "{} → {}",
                            Self::relation_label(rel),
                            &other_id[..8.min(other_id.len())]
                        )
                    })
                    .collect();
                output.push_str(&rel_strs.join(", "));
            }

            output.push('\n');
        }

        output
    }

    // ── Private helpers ──

    fn matches_ref_filter(node: &CanvasNode, filter: &RefFilter) -> bool {
        let actual = match filter.field.as_str() {
            "memory_id" => node.refs.memory_id.as_deref(),
            "artifact_id" => node.refs.artifact_id.as_deref(),
            "task_id" => node.refs.task_id.as_deref(),
            "shot_id" => node.refs.shot_id.as_deref(),
            "scene_id" => node.refs.scene_id.as_deref(),
            "project_id" => node.refs.project_id.as_deref(),
            "agent_id" => node.refs.agent_id.as_deref(),
            "asset_id" => node.refs.asset_id.as_deref(),
            "conversation_id" => node.refs.conversation_id.as_deref(),
            _ => return false,
        };
        actual == Some(filter.value.as_str())
    }

    fn bfs_distances(
        start: &str,
        adjacency: &HashMap<String, Vec<String>>,
        max_depth: u32,
    ) -> HashMap<String, u32> {
        let mut distances = HashMap::new();
        let mut queue = VecDeque::new();
        queue.push_back((start.to_owned(), 0u32));
        distances.insert(start.to_owned(), 0u32);

        while let Some((node_id, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }
            if let Some(neighbors) = adjacency.get(&node_id) {
                for neighbor in neighbors {
                    if !distances.contains_key(neighbor) {
                        distances.insert(neighbor.clone(), depth + 1);
                        queue.push_back((neighbor.clone(), depth + 1));
                    }
                }
            }
        }

        distances
    }

    fn kind_label(kind: CanvasNodeKind) -> &'static str {
        match kind {
            CanvasNodeKind::Scene => "场景",
            CanvasNodeKind::Shot => "镜头",
            CanvasNodeKind::Image => "图片",
            CanvasNodeKind::Video => "视频",
            CanvasNodeKind::Upload => "上传",
            CanvasNodeKind::Note => "笔记",
            CanvasNodeKind::Fact => "事实",
            CanvasNodeKind::Document => "文档",
            CanvasNodeKind::Character => "角色",
            CanvasNodeKind::Prompt => "提示词",
        }
    }

    fn relation_label(rel: &SemanticRelation) -> &'static str {
        match rel {
            SemanticRelation::LeftOf => "在左侧",
            SemanticRelation::RightOf => "在右侧",
            SemanticRelation::Above => "在上方",
            SemanticRelation::Below => "在下方",
            SemanticRelation::ConnectedTo { .. } => "连接",
            SemanticRelation::InSameScene => "同场景",
            SemanticRelation::Nearest { .. } => "相邻",
            SemanticRelation::TemporalBefore => "在前",
            SemanticRelation::TemporalAfter => "在后",
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::canvas::{CanvasNodeRefs, CanvasSize};
    use std::collections::HashMap;

    fn make_node(id: &str, kind: CanvasNodeKind, x: f64, y: f64) -> CanvasNode {
        CanvasNode {
            id: id.to_owned(),
            canvas_id: "c1".to_owned(),
            kind,
            position: CanvasPosition { x, y },
            size: Some(CanvasSize {
                width: 220.0,
                height: 160.0,
            }),
            summary: Some(format!("Node {id}")),
            description: None,
            prompt: None,
            status: None,
            refs: CanvasNodeRefs::default(),
            metadata: HashMap::new(),
            revision: 0,
            created_at: "2024-01-01T00:00:00Z".to_owned(),
            updated_at: "2024-01-01T00:00:00Z".to_owned(),
        }
    }

    fn make_edge(id: &str, source: &str, target: &str, kind: CanvasEdgeKind) -> CanvasEdge {
        CanvasEdge {
            id: id.to_owned(),
            canvas_id: "c1".to_owned(),
            source_node_id: source.to_owned(),
            target_node_id: target.to_owned(),
            kind,
            label: None,
            metadata: HashMap::new(),
            created_at: "2024-01-01T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn spatial_relations_horizontal() {
        let nodes = vec![
            make_node("a", CanvasNodeKind::Image, 0.0, 0.0),
            make_node("b", CanvasNodeKind::Image, 300.0, 0.0),
        ];
        let edges = vec![];
        let relations = CanvasContextBuilder::build_semantic_relations(&nodes, &edges);

        // a is left of b, b is right of a
        assert!(relations
            .iter()
            .any(|(a, b, r)| { a == "a" && b == "b" && matches!(r, SemanticRelation::RightOf) }));
        assert!(relations
            .iter()
            .any(|(a, b, r)| { a == "b" && b == "a" && matches!(r, SemanticRelation::LeftOf) }));
    }

    #[test]
    fn spatial_relations_vertical() {
        let nodes = vec![
            make_node("a", CanvasNodeKind::Image, 0.0, 0.0),
            make_node("b", CanvasNodeKind::Image, 0.0, 300.0),
        ];
        let edges = vec![];
        let relations = CanvasContextBuilder::build_semantic_relations(&nodes, &edges);

        assert!(relations
            .iter()
            .any(|(a, b, r)| { a == "a" && b == "b" && matches!(r, SemanticRelation::Below) }));
        assert!(relations
            .iter()
            .any(|(a, b, r)| { a == "b" && b == "a" && matches!(r, SemanticRelation::Above) }));
    }

    #[test]
    fn edge_based_relations() {
        let nodes = vec![
            make_node("a", CanvasNodeKind::Shot, 0.0, 0.0),
            make_node("b", CanvasNodeKind::Shot, 300.0, 0.0),
        ];
        let edges = vec![make_edge("e1", "a", "b", CanvasEdgeKind::Sequence)];
        let relations = CanvasContextBuilder::build_semantic_relations(&nodes, &edges);

        assert!(relations.iter().any(|(a, b, r)| {
            a == "a" && b == "b" && matches!(r, SemanticRelation::TemporalBefore)
        }));
        assert!(relations.iter().any(|(a, b, r)| {
            a == "a" && b == "b" && matches!(r, SemanticRelation::ConnectedTo { .. })
        }));
    }

    #[test]
    fn select_by_kind_filter() {
        let nodes = vec![
            make_node("a", CanvasNodeKind::Image, 0.0, 0.0),
            make_node("b", CanvasNodeKind::Note, 100.0, 0.0),
            make_node("c", CanvasNodeKind::Image, 200.0, 0.0),
        ];
        let query = ContextQuery {
            kinds: Some(vec![CanvasNodeKind::Image]),
            ..Default::default()
        };
        let budget = ContextBudget::default();
        let result = CanvasContextBuilder::select_relevant_nodes(&nodes, &[], &query, &budget);
        assert_eq!(result.len(), 2);
        assert!(result
            .iter()
            .all(|cn| cn.node.kind == CanvasNodeKind::Image));
    }

    #[test]
    fn select_with_center_node() {
        let nodes = vec![
            make_node("center", CanvasNodeKind::Scene, 0.0, 0.0),
            make_node("near", CanvasNodeKind::Shot, 100.0, 0.0),
            make_node("far", CanvasNodeKind::Shot, 5000.0, 0.0),
        ];
        let edges = vec![make_edge(
            "e1",
            "center",
            "near",
            CanvasEdgeKind::Composition,
        )];
        let query = ContextQuery {
            center_node_id: Some("center".to_owned()),
            include_relations: false,
            max_depth: Some(1),
            ..Default::default()
        };
        let budget = ContextBudget::default();
        let result = CanvasContextBuilder::select_relevant_nodes(&nodes, &edges, &query, &budget);

        // center and near should be selected (connected by edge within depth 1)
        // far is not connected, so not reachable
        assert!(result.iter().any(|cn| cn.node.id == "center"));
        assert!(result.iter().any(|cn| cn.node.id == "near"));
        assert!(!result.iter().any(|cn| cn.node.id == "far"));
    }

    #[test]
    fn format_for_llm_empty() {
        let text = CanvasContextBuilder::format_for_llm(&[]);
        assert!(text.contains("画布为空"));
    }

    #[test]
    fn format_for_llm_with_nodes() {
        let contextual = vec![ContextualNode {
            node: make_node("n1", CanvasNodeKind::Image, 100.0, 200.0),
            relations: vec![],
            relevance_score: 0.8,
        }];
        let text = CanvasContextBuilder::format_for_llm(&contextual);
        assert!(text.contains("图片"));
        assert!(text.contains("Node n1"));
        assert!(text.contains("100"));
    }
}
