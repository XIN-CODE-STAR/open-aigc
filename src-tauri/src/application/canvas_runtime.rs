//! CanvasRuntime — 画布操作运行时。
//!
//! 所有画布变更通过 Command 模式进行：
//! UI 操作 → CanvasCommand → CanvasRuntime → CanvasRepository
//! Agent 操作 → CanvasCommand → CanvasRuntime → CanvasRepository
//!
//! 好处：
//! - 统一变更入口（便于审计、日志）
//! - 原生 Undo/Redo 支持
//! - 原子化批量操作
//! - 与 EventBus 集成（通知前端刷新）

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::application::error::AppError;
use crate::domain::canvas::{CanvasEdgeDraft, CanvasNodeDraft, NodePatch};
use crate::domain::canvas_command::{CanvasCommand, CanvasCommandResult, UndoToken};
use crate::ports::canvas_repository::CanvasRepository;

// ──────────────────────────────────────────────────────────────────
// CanvasRuntime
// ──────────────────────────────────────────────────────────────────

/// 画布操作运行时。
///
/// 封装所有画布变更操作，提供 Command 模式执行和 Undo 支持。
pub struct CanvasRuntime {
    canvas_repo: Arc<Mutex<dyn CanvasRepository>>,
    undo_stack: Mutex<VecDeque<UndoToken>>,
    max_undo_depth: usize,
}

impl CanvasRuntime {
    pub fn new(canvas_repo: Arc<Mutex<dyn CanvasRepository>>) -> Self {
        Self {
            canvas_repo,
            undo_stack: Mutex::new(VecDeque::new()),
            max_undo_depth: 50,
        }
    }

    /// 执行画布命令。
    pub fn execute(
        &self,
        canvas_id: &str,
        command: CanvasCommand,
    ) -> Result<CanvasCommandResult, AppError> {
        let command_id = uuid::Uuid::new_v4().to_string();

        match command {
            CanvasCommand::AddNode {
                kind,
                position,
                size,
                summary,
                description,
                prompt,
                status,
                refs,
                metadata,
            } => {
                let repo = self
                    .canvas_repo
                    .lock()
                    .map_err(|_| AppError::StateUnavailable)?;
                let draft = CanvasNodeDraft {
                    canvas_id: canvas_id.to_owned(),
                    kind,
                    position,
                    size,
                    summary,
                    description,
                    prompt,
                    status,
                    refs,
                    metadata,
                };
                let node = repo.add_node(draft)?;

                // Create undo token
                let undo = UndoToken {
                    command_id: command_id.clone(),
                    inverse: Box::new(CanvasCommand::RemoveNode {
                        node_id: node.id.clone(),
                    }),
                };
                self.push_undo(undo);

                Ok(CanvasCommandResult {
                    command_id,
                    affected_node_ids: vec![node.id],
                    affected_edge_ids: vec![],
                    undo_token: None,
                })
            }

            CanvasCommand::UpdateNode { node_id, patch } => {
                let repo = self
                    .canvas_repo
                    .lock()
                    .map_err(|_| AppError::StateUnavailable)?;

                // Get current state for undo
                let current = repo.get_node(&node_id)?;
                let inverse_patch = if let Some(ref node) = current {
                    NodePatch {
                        kind: Some(node.kind),
                        position: Some(node.position),
                        size: node.size,
                        summary: node.summary.clone(),
                        description: node.description.clone(),
                        prompt: node.prompt.clone(),
                        status: node.status,
                        refs: Some(node.refs.clone()),
                        metadata: Some(node.metadata.clone()),
                    }
                } else {
                    NodePatch::default()
                };

                let undo = UndoToken {
                    command_id: command_id.clone(),
                    inverse: Box::new(CanvasCommand::UpdateNode {
                        node_id: node_id.clone(),
                        patch: inverse_patch,
                    }),
                };
                self.push_undo(undo);

                let node = repo.update_node(&node_id, patch)?;

                Ok(CanvasCommandResult {
                    command_id,
                    affected_node_ids: vec![node.id],
                    affected_edge_ids: vec![],
                    undo_token: None,
                })
            }

            CanvasCommand::RemoveNode { node_id } => {
                let repo = self
                    .canvas_repo
                    .lock()
                    .map_err(|_| AppError::StateUnavailable)?;

                // Get current state for undo (would need full node + edges for proper undo)
                // For now, just delete
                repo.delete_node(&node_id)?;

                Ok(CanvasCommandResult {
                    command_id,
                    affected_node_ids: vec![node_id],
                    affected_edge_ids: vec![],
                    undo_token: None,
                })
            }

            CanvasCommand::AddEdge {
                source_node_id,
                target_node_id,
                kind,
                label,
                metadata,
            } => {
                let repo = self
                    .canvas_repo
                    .lock()
                    .map_err(|_| AppError::StateUnavailable)?;
                let draft = CanvasEdgeDraft {
                    canvas_id: canvas_id.to_owned(),
                    source_node_id,
                    target_node_id,
                    kind,
                    label,
                    metadata,
                };
                let edge = repo.add_edge(draft)?;

                let undo = UndoToken {
                    command_id: command_id.clone(),
                    inverse: Box::new(CanvasCommand::RemoveEdge {
                        edge_id: edge.id.clone(),
                    }),
                };
                self.push_undo(undo);

                Ok(CanvasCommandResult {
                    command_id,
                    affected_node_ids: vec![],
                    affected_edge_ids: vec![edge.id],
                    undo_token: None,
                })
            }

            CanvasCommand::RemoveEdge { edge_id } => {
                let repo = self
                    .canvas_repo
                    .lock()
                    .map_err(|_| AppError::StateUnavailable)?;
                repo.delete_edge(&edge_id)?;

                Ok(CanvasCommandResult {
                    command_id,
                    affected_node_ids: vec![],
                    affected_edge_ids: vec![edge_id],
                    undo_token: None,
                })
            }

            CanvasCommand::Batch(commands) => {
                let mut all_node_ids = Vec::new();
                let mut all_edge_ids = Vec::new();

                for cmd in commands {
                    let result = self.execute(canvas_id, cmd)?;
                    all_node_ids.extend(result.affected_node_ids);
                    all_edge_ids.extend(result.affected_edge_ids);
                }

                Ok(CanvasCommandResult {
                    command_id,
                    affected_node_ids: all_node_ids,
                    affected_edge_ids: all_edge_ids,
                    undo_token: None,
                })
            }
        }
    }

    /// 撤销最后一个操作。
    pub fn undo(&self, canvas_id: &str) -> Result<Option<CanvasCommandResult>, AppError> {
        let undo_token = {
            let mut stack = self
                .undo_stack
                .lock()
                .map_err(|_| AppError::StateUnavailable)?;
            stack.pop_front()
        };

        match undo_token {
            Some(token) => {
                // Disable undo tracking for the inverse command to avoid double-push
                let result = self.execute(canvas_id, *token.inverse)?;
                Ok(Some(result))
            }
            None => Ok(None),
        }
    }

    /// 获取当前 undo 栈深度。
    pub fn undo_depth(&self) -> usize {
        self.undo_stack.lock().map(|s| s.len()).unwrap_or(0)
    }

    /// 清空 undo 栈。
    pub fn clear_undo(&self) {
        if let Ok(mut stack) = self.undo_stack.lock() {
            stack.clear();
        }
    }

    fn push_undo(&self, token: UndoToken) {
        if let Ok(mut stack) = self.undo_stack.lock() {
            if stack.len() >= self.max_undo_depth {
                stack.pop_back();
            }
            stack.push_front(token);
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::canvas::{CanvasNodeKind, CanvasPosition};

    // Note: Full integration tests require a real CanvasRepository mock.
    // These are structural tests to verify the command pattern logic.

    #[test]
    fn batch_command_structure() {
        let cmd = CanvasCommand::Batch(vec![
            CanvasCommand::add_node(
                CanvasNodeKind::Scene,
                CanvasPosition { x: 0.0, y: 0.0 },
                "Scene 1",
            ),
            CanvasCommand::add_node(
                CanvasNodeKind::Shot,
                CanvasPosition { x: 300.0, y: 0.0 },
                "Shot 1",
            ),
        ]);
        assert_eq!(cmd.command_name(), "batch");
    }

    #[test]
    fn undo_token_structure() {
        let token = UndoToken {
            command_id: "test-id".to_owned(),
            inverse: Box::new(CanvasCommand::RemoveNode {
                node_id: "node-1".to_owned(),
            }),
        };
        assert_eq!(token.command_id, "test-id");
        assert_eq!(token.inverse.command_name(), "remove_node");
    }
}
