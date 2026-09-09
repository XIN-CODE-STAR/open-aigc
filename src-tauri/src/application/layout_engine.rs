//! LayoutEngine — 画布节点布局算法。
//!
//! 抽象化的布局引擎，替代 MemoryCanvasPanel.vue 中硬编码的 `x + 260` 定位。
//! 支持多种布局策略，可在前后端共享逻辑。
//!
//! 布局策略：
//! - Timeline: 左到右水平时间线，场景组纵向排列
//! - Grid: 固定网格布局（scene × shot）
//! - Hierarchical: 树形层次结构

use crate::domain::canvas::CanvasPosition;

// ──────────────────────────────────────────────────────────────────
// LayoutStrategy
// ──────────────────────────────────────────────────────────────────

/// 布局策略枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
// LayoutOptions
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
// GroupLayout — 一组节点的布局描述
// ──────────────────────────────────────────────────────────────────

/// 一组节点的布局描述（如一个场景及其镜头）。
#[derive(Debug, Clone)]
pub struct GroupLayout {
    /// 组的标签（如场景标题）。
    pub label: String,
    /// 组内节点数量。
    pub child_count: usize,
}

// ──────────────────────────────────────────────────────────────────
// LayoutEngine
// ──────────────────────────────────────────────────────────────────

/// 画布布局引擎。
///
/// 纯计算模块，不涉及任何 I/O。给定组结构和布局策略，
/// 计算每个节点的 (x, y) 位置。
pub struct LayoutEngine;

impl LayoutEngine {
    /// 为多个组计算布局位置。
    ///
    /// `groups` 描述每个组的子节点数量。
    /// 返回 `positions[group_idx][child_idx]`。
    pub fn layout_groups(
        groups: &[GroupLayout],
        strategy: LayoutStrategy,
        options: &LayoutOptions,
    ) -> Vec<Vec<CanvasPosition>> {
        match strategy {
            LayoutStrategy::Timeline => Self::timeline_layout(groups, options),
            LayoutStrategy::Grid => Self::grid_layout(groups, options),
            LayoutStrategy::Hierarchical => Self::hierarchical_layout(groups, options),
        }
    }

    /// 为单个组（如新增的节点）计算位置，基于现有节点的偏移。
    ///
    /// 替代 MemoryCanvasPanel.vue 中硬编码的 `lastNode.x + 260`。
    pub fn next_position(
        existing_positions: &[CanvasPosition],
        options: &LayoutOptions,
    ) -> CanvasPosition {
        if existing_positions.is_empty() {
            return CanvasPosition {
                x: options.start_x,
                y: options.start_y,
            };
        }

        // Find the rightmost node
        let max_x = existing_positions
            .iter()
            .map(|p| p.x)
            .fold(f64::MIN, f64::max);

        // Stagger Y every 3 nodes
        let count = existing_positions.len();
        let y_offset = ((count / 3) as f64) * 80.0 * if count % 6 < 3 { 1.0 } else { -1.0 };

        CanvasPosition {
            x: max_x + options.node_width + options.horizontal_gap,
            y: options.start_y + y_offset,
        }
    }

    // ── Timeline layout ──

    /// Timeline: scenes stacked vertically, shots go left to right within each scene.
    fn timeline_layout(groups: &[GroupLayout], opts: &LayoutOptions) -> Vec<Vec<CanvasPosition>> {
        let mut result = Vec::new();

        for (group_idx, group) in groups.iter().enumerate() {
            let group_y = opts.start_y + (group_idx as f64) * (opts.node_height + opts.group_gap);
            let mut child_positions = Vec::new();

            for child_idx in 0..group.child_count {
                let x = opts.start_x + (child_idx as f64) * (opts.node_width + opts.horizontal_gap);
                let y = group_y;
                child_positions.push(CanvasPosition { x, y });
            }

            result.push(child_positions);
        }

        result
    }

    // ── Grid layout ──

    /// Grid: fixed grid with consistent row height per group.
    fn grid_layout(groups: &[GroupLayout], opts: &LayoutOptions) -> Vec<Vec<CanvasPosition>> {
        let max_children = groups.iter().map(|g| g.child_count).max().unwrap_or(0);
        let mut result = Vec::new();

        for (group_idx, group) in groups.iter().enumerate() {
            let mut child_positions = Vec::new();

            for child_idx in 0..group.child_count {
                let x = opts.start_x + (child_idx as f64) * (opts.node_width + opts.horizontal_gap);
                let y = opts.start_y + (group_idx as f64) * (opts.node_height + opts.vertical_gap);
                child_positions.push(CanvasPosition { x, y });
            }

            result.push(child_positions);
        }

        let _ = max_children; // used for future centering
        result
    }

    // ── Hierarchical layout ──

    /// Hierarchical: groups centered vertically, children spread to the right.
    fn hierarchical_layout(
        groups: &[GroupLayout],
        opts: &LayoutOptions,
    ) -> Vec<Vec<CanvasPosition>> {
        let total_groups = groups.len() as f64;
        let center_y = opts.start_y + (total_groups * (opts.node_height + opts.vertical_gap)) / 2.0;
        let mut result = Vec::new();

        for (group_idx, group) in groups.iter().enumerate() {
            let group_y = center_y
                + (group_idx as f64 - total_groups / 2.0)
                    * (opts.node_height + opts.vertical_gap)
                    * 1.5;
            let group_x = opts.start_x;

            let total_children = group.child_count as f64;
            let mut child_positions = Vec::new();

            for child_idx in 0..group.child_count {
                let x = group_x + opts.node_width + opts.horizontal_gap * 2.0;
                let y = group_y
                    + (child_idx as f64 - total_children / 2.0)
                        * (opts.node_height + opts.vertical_gap * 0.5);
                child_positions.push(CanvasPosition { x, y });
            }

            result.push(child_positions);
        }

        result
    }
}

// ──────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_groups() -> Vec<GroupLayout> {
        vec![
            GroupLayout {
                label: "Scene 1".to_owned(),
                child_count: 3,
            },
            GroupLayout {
                label: "Scene 2".to_owned(),
                child_count: 2,
            },
        ]
    }

    #[test]
    fn timeline_layout_basic() {
        let groups = test_groups();
        let opts = LayoutOptions::default();
        let positions = LayoutEngine::layout_groups(&groups, LayoutStrategy::Timeline, &opts);

        assert_eq!(positions.len(), 2);
        assert_eq!(positions[0].len(), 3);
        assert_eq!(positions[1].len(), 2);

        // Scene 2 should be below Scene 1
        assert!(positions[1][0].y > positions[0][0].y);

        // Shots within a scene go left to right
        assert!(positions[0][1].x > positions[0][0].x);
        assert!(positions[0][2].x > positions[0][1].x);
    }

    #[test]
    fn grid_layout_basic() {
        let groups = test_groups();
        let opts = LayoutOptions::default();
        let positions = LayoutEngine::layout_groups(&groups, LayoutStrategy::Grid, &opts);

        // All shots in scene 1 should be at same Y
        assert_eq!(positions[0][0].y, positions[0][1].y);
        assert_eq!(positions[0][1].y, positions[0][2].y);

        // Scene 2 row should be below scene 1
        assert!(positions[1][0].y > positions[0][0].y);
    }

    #[test]
    fn hierarchical_layout_basic() {
        let groups = test_groups();
        let opts = LayoutOptions::default();
        let positions = LayoutEngine::layout_groups(&groups, LayoutStrategy::Hierarchical, &opts);

        // Children should be to the right of groups
        assert!(positions[0][0].x > opts.start_x);
    }

    #[test]
    fn next_position_empty() {
        let opts = LayoutOptions::default();
        let pos = LayoutEngine::next_position(&[], &opts);
        assert_eq!(pos.x, opts.start_x);
        assert_eq!(pos.y, opts.start_y);
    }

    #[test]
    fn next_position_offset() {
        let opts = LayoutOptions::default();
        let existing = vec![
            CanvasPosition { x: 100.0, y: 50.0 },
            CanvasPosition { x: 360.0, y: 50.0 },
        ];
        let pos = LayoutEngine::next_position(&existing, &opts);
        // Should be to the right of the rightmost node
        assert!(pos.x > 360.0);
    }
}
