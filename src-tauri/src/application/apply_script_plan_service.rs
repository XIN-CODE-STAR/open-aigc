//! ApplyScriptPlanService — ScriptPlan → MangaProject + CanvasNodes。
//!
//! 这是连接"脚本解析"和"项目持久化"的关键服务。
//! 消费 ScriptPlan，创建 MangaProject + Scenes + Shots + CanvasNodes + CanvasEdges。
//!
//! 数据流：
//! ScriptPlan → ApplyScriptPlanService → MangaProject + Scenes + Shots
//!                                      → CanvasNodes (scene/shot 类型)
//!                                      → CanvasEdges (composition/sequence)
//!
//! 设计原则：
//! - 通过 MangaService 创建 MangaProject 层级（不直接操作 MangaRepository）
//! - 通过 CanvasRepository 创建画布节点和边
//! - 使用 LayoutEngine 自动布局节点位置
//! - 返回 ApplyResult 包含所有创建的 ID

use std::sync::{Arc, Mutex};

use crate::application::error::AppError;
use crate::domain::canvas::{
    CanvasEdgeDraft, CanvasEdgeKind, CanvasNodeDraft, CanvasNodeKind, CanvasNodeRefs,
    CanvasNodeStatus, CanvasPosition, CanvasSize,
};
use crate::domain::script_plan::ScriptPlan;
use crate::ports::canvas_repository::CanvasRepository;

// ──────────────────────────────────────────────────────────────────
// LayoutStrategy — 布局策略
// ──────────────────────────────────────────────────────────────────

/// 画布节点布局策略。
#[derive(Debug, Clone, Copy)]
pub enum LayoutStrategy {
    /// 左到右水平时间线（默认）。
    Timeline,
    /// 网格布局（按 scene × shot）。
    Grid,
    /// 树形层次结构。
    Hierarchical,
}

impl Default for LayoutStrategy {
    fn default() -> Self {
        Self::Timeline
    }
}

// ──────────────────────────────────────────────────────────────────
// LayoutOptions — 布局参数
// ──────────────────────────────────────────────────────────────────

/// 布局参数。
#[derive(Debug, Clone)]
pub struct LayoutOptions {
    pub start_x: f64,
    pub start_y: f64,
    pub node_width: f64,
    pub node_height: f64,
    pub horizontal_gap: f64,
    pub vertical_gap: f64,
    pub group_gap: f64,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            start_x: 50.0,
            start_y: 50.0,
            node_width: 220.0,
            node_height: 160.0,
            horizontal_gap: 40.0,
            vertical_gap: 40.0,
            group_gap: 80.0,
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// ApplyResult — 应用结果
// ──────────────────────────────────────────────────────────────────

/// 应用 ScriptPlan 的结果。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub canvas_id: String,
    pub scene_node_ids: Vec<String>,
    pub shot_node_ids: Vec<String>,
    pub edge_ids: Vec<String>,
    pub total_nodes: usize,
    pub total_edges: usize,
}

// ──────────────────────────────────────────────────────────────────
// ApplyScriptPlanService
// ──────────────────────────────────────────────────────────────────

/// ScriptPlan 应用服务。
///
/// 将 ScriptPlan 转化为画布上的节点和边。
/// 注意：此服务只负责画布侧的创建，MangaProject/Scene/Shot 的持久化
/// 需要由调用方（Agent 工具或 IPC 命令）通过 MangaService 完成。
pub struct ApplyScriptPlanService {
    canvas_repo: Arc<Mutex<dyn CanvasRepository>>,
}

impl ApplyScriptPlanService {
    pub fn new(canvas_repo: Arc<Mutex<dyn CanvasRepository>>) -> Self {
        Self { canvas_repo }
    }

    /// 应用 ScriptPlan 到画布。
    ///
    /// 创建场景节点、镜头节点、组成边和时序边。
    /// 不创建 MangaProject/Scene/Shot（由调用方负责）。
    pub fn apply(
        &self,
        workspace_id: &str,
        canvas_id: &str,
        plan: &ScriptPlan,
        strategy: LayoutStrategy,
    ) -> Result<ApplyResult, AppError> {
        let repo = self
            .canvas_repo
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let opts = LayoutOptions::default();

        let mut scene_node_ids = Vec::new();
        let mut shot_node_ids = Vec::new();
        let mut all_node_drafts = Vec::new();
        let mut all_edge_drafts = Vec::new();

        // Compute layout positions
        let positions = Self::compute_layout(plan, strategy, &opts);

        // Create scene nodes and shot nodes
        for (scene_idx, scene) in plan.scenes.iter().enumerate() {
            let scene_pos = positions.scenes[scene_idx];

            let scene_draft = CanvasNodeDraft {
                canvas_id: canvas_id.to_owned(),
                kind: CanvasNodeKind::Scene,
                position: scene_pos,
                size: Some(CanvasSize {
                    width: opts.node_width,
                    height: opts.node_height,
                }),
                summary: Some(scene.title.clone()),
                description: scene.summary.clone(),
                prompt: None,
                status: Some(CanvasNodeStatus::Draft),
                refs: CanvasNodeRefs {
                    conversation_id: plan.conversation_id.clone(),
                    ..Default::default()
                },
                metadata: serde_json::json!({
                    "sceneIndex": scene.index,
                    "location": scene.location,
                    "shotCount": scene.shots.len(),
                    "scriptPlanId": plan.id,
                })
                .as_object()
                .map(|m| m.into_iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default(),
            };
            all_node_drafts.push(scene_draft);

            for (shot_idx, shot) in scene.shots.iter().enumerate() {
                let shot_pos = positions.shots[scene_idx][shot_idx];

                let mut metadata = serde_json::json!({
                    "shotIndex": shot.index,
                    "shotType": shot.shot_type,
                    "cameraMotion": shot.camera_motion,
                    "scriptPlanId": plan.id,
                });
                if !shot.character_refs.is_empty() {
                    metadata["characterRefs"] = serde_json::json!(shot.character_refs);
                }

                let shot_draft = CanvasNodeDraft {
                    canvas_id: canvas_id.to_owned(),
                    kind: CanvasNodeKind::Shot,
                    position: shot_pos,
                    size: Some(CanvasSize {
                        width: opts.node_width,
                        height: opts.node_height,
                    }),
                    summary: Some(shot.description.chars().take(100).collect()),
                    description: Some(shot.description.clone()),
                    prompt: shot.visual_intent.clone(),
                    status: Some(CanvasNodeStatus::Draft),
                    refs: CanvasNodeRefs {
                        conversation_id: plan.conversation_id.clone(),
                        ..Default::default()
                    },
                    metadata: metadata
                        .as_object()
                        .map(|m| m.into_iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                        .unwrap_or_default(),
                };
                all_node_drafts.push(shot_draft);
            }
        }

        // Batch create all nodes
        let created_nodes = repo.add_nodes_batch(all_node_drafts)?;

        // Split created nodes into scene and shot nodes
        for node in &created_nodes {
            match node.kind {
                CanvasNodeKind::Scene => scene_node_ids.push(node.id.clone()),
                CanvasNodeKind::Shot => shot_node_ids.push(node.id.clone()),
                _ => {}
            }
        }

        // Create edges: composition (scene → shot) and sequence (shot → shot)
        let mut shot_idx = 0;
        for (scene_idx, scene) in plan.scenes.iter().enumerate() {
            let scene_node_id = &scene_node_ids[scene_idx];

            for _ in 0..scene.shots.len() {
                let shot_node_id = &shot_node_ids[shot_idx];

                // Composition edge: scene contains shot
                all_edge_drafts.push(CanvasEdgeDraft {
                    canvas_id: canvas_id.to_owned(),
                    source_node_id: shot_node_id.clone(),
                    target_node_id: scene_node_id.clone(),
                    kind: CanvasEdgeKind::Composition,
                    label: None,
                    metadata: serde_json::json!({"auto": true})
                        .as_object()
                        .map(|m| m.into_iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                        .unwrap_or_default(),
                });

                // Sequence edge: previous shot → current shot
                if shot_idx > 0 {
                    let prev_shot_id = &shot_node_ids[shot_idx - 1];
                    all_edge_drafts.push(CanvasEdgeDraft {
                        canvas_id: canvas_id.to_owned(),
                        source_node_id: prev_shot_id.clone(),
                        target_node_id: shot_node_id.clone(),
                        kind: CanvasEdgeKind::Sequence,
                        label: None,
                        metadata: serde_json::json!({"auto": true})
                            .as_object()
                            .map(|m| m.into_iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                            .unwrap_or_default(),
                    });
                }

                shot_idx += 1;
            }
        }

        // Batch create all edges
        let created_edges = repo.add_edges_batch(all_edge_drafts)?;
        let edge_ids: Vec<String> = created_edges.iter().map(|e| e.id.clone()).collect();

        let total_nodes = created_nodes.len();
        let total_edges = created_edges.len();

        Ok(ApplyResult {
            canvas_id: canvas_id.to_owned(),
            scene_node_ids,
            shot_node_ids,
            edge_ids,
            total_nodes,
            total_edges,
        })
    }

    // ── Layout computation ──

    /// Compute positions for all scene and shot nodes.
    fn compute_layout(
        plan: &ScriptPlan,
        strategy: LayoutStrategy,
        opts: &LayoutOptions,
    ) -> LayoutPositions {
        match strategy {
            LayoutStrategy::Timeline => Self::layout_timeline(plan, opts),
            LayoutStrategy::Grid => Self::layout_grid(plan, opts),
            LayoutStrategy::Hierarchical => Self::layout_hierarchical(plan, opts),
        }
    }

    /// Timeline layout: scenes stacked vertically, shots go left to right.
    fn layout_timeline(plan: &ScriptPlan, opts: &LayoutOptions) -> LayoutPositions {
        let mut scenes = Vec::new();
        let mut shots = Vec::new();

        for (scene_idx, scene) in plan.scenes.iter().enumerate() {
            let scene_y = opts.start_y + (scene_idx as f64) * (opts.node_height + opts.group_gap);
            let scene_x = opts.start_x;
            scenes.push(CanvasPosition {
                x: scene_x,
                y: scene_y,
            });

            let mut scene_shots = Vec::new();
            for (shot_idx, _) in scene.shots.iter().enumerate() {
                let shot_x = scene_x
                    + opts.node_width
                    + opts.horizontal_gap
                    + (shot_idx as f64) * (opts.node_width + opts.horizontal_gap);
                let shot_y = scene_y;
                scene_shots.push(CanvasPosition {
                    x: shot_x,
                    y: shot_y,
                });
            }
            shots.push(scene_shots);
        }

        LayoutPositions { scenes, shots }
    }

    /// Grid layout: fixed grid by scene index × shot index.
    fn layout_grid(plan: &ScriptPlan, opts: &LayoutOptions) -> LayoutPositions {
        let mut scenes = Vec::new();
        let mut shots = Vec::new();

        for (scene_idx, scene) in plan.scenes.iter().enumerate() {
            let scene_x = opts.start_x;
            let scene_y =
                opts.start_y + (scene_idx as f64) * (opts.node_height + opts.vertical_gap);
            scenes.push(CanvasPosition {
                x: scene_x,
                y: scene_y,
            });

            let mut scene_shots = Vec::new();
            for (shot_idx, _) in scene.shots.iter().enumerate() {
                let shot_x = opts.start_x
                    + (opts.node_width + opts.horizontal_gap)
                    + (shot_idx as f64) * (opts.node_width + opts.horizontal_gap);
                let shot_y = scene_y;
                scene_shots.push(CanvasPosition {
                    x: shot_x,
                    y: shot_y,
                });
            }
            shots.push(scene_shots);
        }

        LayoutPositions { scenes, shots }
    }

    /// Hierarchical layout: tree with project root → scene branches → shot leaves.
    fn layout_hierarchical(plan: &ScriptPlan, opts: &LayoutOptions) -> LayoutPositions {
        let mut scenes = Vec::new();
        let mut shots = Vec::new();

        let center_x = opts.start_x + 300.0;
        let total_scenes = plan.scenes.len() as f64;
        let scene_spacing = (opts.node_height + opts.vertical_gap) * 1.5;

        for (scene_idx, scene) in plan.scenes.iter().enumerate() {
            let scene_y = opts.start_y + (scene_idx as f64 - total_scenes / 2.0) * scene_spacing;
            scenes.push(CanvasPosition {
                x: center_x,
                y: scene_y,
            });

            let total_shots = scene.shots.len() as f64;
            let shot_spacing = opts.node_height + opts.vertical_gap * 0.5;

            let mut scene_shots = Vec::new();
            for (shot_idx, _) in scene.shots.iter().enumerate() {
                let shot_x = center_x + opts.node_width + opts.horizontal_gap * 2.0;
                let shot_y = scene_y + (shot_idx as f64 - total_shots / 2.0) * shot_spacing;
                scene_shots.push(CanvasPosition {
                    x: shot_x,
                    y: shot_y,
                });
            }
            shots.push(scene_shots);
        }

        LayoutPositions { scenes, shots }
    }
}

// ──────────────────────────────────────────────────────────────────
// LayoutPositions — computed positions
// ──────────────────────────────────────────────────────────────────

struct LayoutPositions {
    scenes: Vec<CanvasPosition>,
    shots: Vec<Vec<CanvasPosition>>, // shots[scene_idx][shot_idx]
}

// ──────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::script_plan::{PlannedScene, PlannedShot, ScriptPlanDraft};

    fn make_plan() -> ScriptPlan {
        ScriptPlanDraft {
            conversation_id: Some("test-conv".to_owned()),
            title: Some("Test Script".to_owned()),
            style: None,
            scenes: vec![
                PlannedScene {
                    index: 1,
                    title: "Scene 1".to_owned(),
                    summary: None,
                    location: Some("内景".to_owned()),
                    shots: vec![
                        PlannedShot {
                            index: 1,
                            description: "Opening shot".to_owned(),
                            shot_type: Some("wide".to_owned()),
                            camera_motion: Some("static".to_owned()),
                            visual_intent: Some("test prompt".to_owned()),
                            duration: None,
                            character_refs: vec![],
                            style_override: None,
                        },
                        PlannedShot {
                            index: 2,
                            description: "Close up".to_owned(),
                            shot_type: Some("close-up".to_owned()),
                            camera_motion: Some("dolly-in".to_owned()),
                            visual_intent: Some("test prompt 2".to_owned()),
                            duration: None,
                            character_refs: vec!["hero".to_owned()],
                            style_override: None,
                        },
                    ],
                },
                PlannedScene {
                    index: 2,
                    title: "Scene 2".to_owned(),
                    summary: None,
                    location: Some("外景".to_owned()),
                    shots: vec![PlannedShot {
                        index: 1,
                        description: "Final shot".to_owned(),
                        shot_type: Some("medium".to_owned()),
                        camera_motion: Some("tracking".to_owned()),
                        visual_intent: Some("final prompt".to_owned()),
                        duration: None,
                        character_refs: vec![],
                        style_override: None,
                    }],
                },
            ],
        }
        .try_into_plan()
        .unwrap()
    }

    #[test]
    fn timeline_layout_positions() {
        let plan = make_plan();
        let opts = LayoutOptions::default();
        let positions = ApplyScriptPlanService::layout_timeline(&plan, &opts);

        assert_eq!(positions.scenes.len(), 2);
        assert_eq!(positions.shots.len(), 2);
        assert_eq!(positions.shots[0].len(), 2);
        assert_eq!(positions.shots[1].len(), 1);

        // Scene 2 should be below Scene 1
        assert!(positions.scenes[1].y > positions.scenes[0].y);

        // Shot 2 should be right of Shot 1
        assert!(positions.shots[0][1].x > positions.shots[0][0].x);
    }

    #[test]
    fn grid_layout_positions() {
        let plan = make_plan();
        let opts = LayoutOptions::default();
        let positions = ApplyScriptPlanService::layout_grid(&plan, &opts);

        assert_eq!(positions.scenes.len(), 2);
        // Grid: shots at consistent Y per scene
        assert_eq!(positions.shots[0][0].y, positions.shots[0][1].y);
    }

    #[test]
    fn hierarchical_layout_positions() {
        let plan = make_plan();
        let opts = LayoutOptions::default();
        let positions = ApplyScriptPlanService::layout_hierarchical(&plan, &opts);

        // Hierarchical: scenes vertically centered, shots spread from scene position
        assert_eq!(positions.scenes.len(), 2);
        assert_eq!(positions.shots[0].len(), 2);
    }
}
