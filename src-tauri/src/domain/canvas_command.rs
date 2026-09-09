//! Canvas Command 模式 — 画布操作的命令化封装。
#![allow(dead_code)]
//!
//! 所有画布变更（无论是 UI 操作还是 Agent 操作）都通过 Command 进行。
//! 好处：
//! - 统一的变更入口（便于审计、日志）
//! - 原生 Undo/Redo 支持
//! - 原子化批量操作
//! - 与 CanvasRuntime 解耦（Runtime 可以做 debounce、optimistic update）

use serde::{Deserialize, Serialize};

use super::canvas::{
    CanvasEdgeKind, CanvasNodeKind, CanvasNodeRefs, CanvasNodeStatus, CanvasPosition, CanvasSize,
    NodePatch,
};

// ──────────────────────────────────────────────────────────────────
// CanvasCommand — 画布操作命令
// ──────────────────────────────────────────────────────────────────

/// 画布操作命令。
///
/// 所有画布变更都通过此枚举表示。CanvasRuntime 负责执行并产生结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum CanvasCommand {
    /// 添加节点。
    AddNode {
        kind: CanvasNodeKind,
        position: CanvasPosition,
        size: Option<CanvasSize>,
        summary: Option<String>,
        description: Option<String>,
        prompt: Option<String>,
        status: Option<CanvasNodeStatus>,
        refs: CanvasNodeRefs,
        metadata: std::collections::HashMap<String, serde_json::Value>,
    },

    /// 更新节点（部分字段）。
    UpdateNode { node_id: String, patch: NodePatch },

    /// 删除节点（级联删除关联的边）。
    RemoveNode { node_id: String },

    /// 添加边。
    AddEdge {
        source_node_id: String,
        target_node_id: String,
        kind: CanvasEdgeKind,
        label: Option<String>,
        metadata: std::collections::HashMap<String, serde_json::Value>,
    },

    /// 删除边。
    RemoveEdge { edge_id: String },

    /// 批量操作（原子执行，任一失败则整体回滚）。
    Batch(Vec<CanvasCommand>),
}

impl CanvasCommand {
    /// 便捷构造：添加节点。
    pub fn add_node(
        kind: CanvasNodeKind,
        position: CanvasPosition,
        summary: impl Into<String>,
    ) -> Self {
        Self::AddNode {
            kind,
            position,
            size: None,
            summary: Some(summary.into()),
            description: None,
            prompt: None,
            status: None,
            refs: CanvasNodeRefs::default(),
            metadata: std::collections::HashMap::new(),
        }
    }

    /// 便捷构造：更新节点状态。
    pub fn set_status(node_id: impl Into<String>, status: CanvasNodeStatus) -> Self {
        Self::UpdateNode {
            node_id: node_id.into(),
            patch: NodePatch {
                status: Some(status),
                ..Default::default()
            },
        }
    }

    /// 便捷构造：更新节点位置（拖拽后）。
    pub fn move_node(node_id: impl Into<String>, position: CanvasPosition) -> Self {
        Self::UpdateNode {
            node_id: node_id.into(),
            patch: NodePatch {
                position: Some(position),
                ..Default::default()
            },
        }
    }

    /// 便捷构造：添加边。
    pub fn add_edge(
        source: impl Into<String>,
        target: impl Into<String>,
        kind: CanvasEdgeKind,
    ) -> Self {
        Self::AddEdge {
            source_node_id: source.into(),
            target_node_id: target.into(),
            kind,
            label: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// 返回命令的可读名称（用于日志和审计）。
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::AddNode { .. } => "add_node",
            Self::UpdateNode { .. } => "update_node",
            Self::RemoveNode { .. } => "remove_node",
            Self::AddEdge { .. } => "add_edge",
            Self::RemoveEdge { .. } => "remove_edge",
            Self::Batch(_) => "batch",
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// CanvasCommandResult — 命令执行结果
// ──────────────────────────────────────────────────────────────────

/// 画布命令执行结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasCommandResult {
    /// 命令执行的唯一 ID。
    pub command_id: String,
    /// 受影响的节点 ID。
    pub affected_node_ids: Vec<String>,
    /// 受影响的边 ID。
    pub affected_edge_ids: Vec<String>,
    /// 可选的 Undo Token（用于撤销操作）。
    pub undo_token: Option<UndoToken>,
}

// ──────────────────────────────────────────────────────────────────
// UndoToken — 撤销支持
// ──────────────────────────────────────────────────────────────────

/// 撤销令牌 — 包含反向命令。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoToken {
    /// 对应的命令 ID。
    pub command_id: String,
    /// 反向命令（执行此命令等于撤销原命令）。
    pub inverse: Box<CanvasCommand>,
}

// ──────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_node_command() {
        let cmd = CanvasCommand::add_node(
            CanvasNodeKind::Image,
            CanvasPosition { x: 100.0, y: 200.0 },
            "Test node",
        );
        assert_eq!(cmd.command_name(), "add_node");
    }

    #[test]
    fn set_status_command() {
        let cmd = CanvasCommand::set_status("node-1", CanvasNodeStatus::Generating);
        assert_eq!(cmd.command_name(), "update_node");
        if let CanvasCommand::UpdateNode { node_id, patch } = &cmd {
            assert_eq!(node_id, "node-1");
            assert_eq!(patch.status, Some(CanvasNodeStatus::Generating));
        } else {
            panic!("expected UpdateNode");
        }
    }

    #[test]
    fn move_node_command() {
        let cmd = CanvasCommand::move_node("node-1", CanvasPosition { x: 50.0, y: 60.0 });
        assert_eq!(cmd.command_name(), "update_node");
    }

    #[test]
    fn add_edge_command() {
        let cmd = CanvasCommand::add_edge("n1", "n2", CanvasEdgeKind::Sequence);
        assert_eq!(cmd.command_name(), "add_edge");
    }

    #[test]
    fn batch_command() {
        let cmd = CanvasCommand::Batch(vec![
            CanvasCommand::add_node(CanvasNodeKind::Scene, CanvasPosition::default(), "Scene 1"),
            CanvasCommand::add_node(CanvasNodeKind::Shot, CanvasPosition::default(), "Shot 1"),
        ]);
        assert_eq!(cmd.command_name(), "batch");
    }

    #[test]
    fn command_serialization() {
        let cmd = CanvasCommand::add_node(
            CanvasNodeKind::Character,
            CanvasPosition::default(),
            "Agent running",
        );
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("add-node"));
        let back: CanvasCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(back.command_name(), "add_node");
    }
}
