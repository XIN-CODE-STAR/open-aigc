#![allow(dead_code)]
//! Execution Runtime：执行运行时控制器。
//!
//! 包装 ExecutionContext，提供语义化操作接口（complete_step / add_artifact / fail_step）。
//! Executor 只通过这些接口与 Runtime 交互，不直接操作 Context 字段。
//! 未来 Checkpoint / SQLite / 事件推送只需改 Runtime，Executor 不动。

use crate::domain::execution::{
    Artifact, ArtifactStore, ExecutionContext, ExecutionResult, ExecutionStatus,
};

/// 步骤执行结果（Executor → Runtime 的通信协议）。
#[allow(clippy::large_enum_variant)] // Success 含完整 Artifact，失败/跳过路径无性能影响
#[derive(Debug, Clone)]
pub enum StepOutcome {
    /// 步骤成功，产生一个资产。
    Success { artifact: Artifact },
    /// 步骤失败。
    Failed { reason: String },
    /// 步骤跳过（不需要执行）。
    Skipped,
}

/// 执行运行时控制器。
pub struct ExecutionRuntime {
    context: ExecutionContext,
    /// 每步的结果记录（step_index → outcome）。
    step_results: Vec<(u8, StepOutcome)>,
    /// 执行开始时间戳（用于计算 duration）。
    start_instant: std::time::Instant,
}

impl ExecutionRuntime {
    /// 创建运行时。
    pub fn new(plan_id: String, workspace_id: String) -> Self {
        Self {
            context: ExecutionContext::new(plan_id, workspace_id),
            step_results: Vec::new(),
            start_instant: std::time::Instant::now(),
        }
    }

    /// 当前步骤索引。
    pub fn current_step(&self) -> usize {
        self.context.current_step
    }

    /// 所属工作区 ID。
    pub fn workspace_id(&self) -> &str {
        &self.context.workspace_id
    }

    /// 获取共享变量。
    pub fn get_variable(&self, key: &str) -> Option<&serde_json::Value> {
        self.context.get_variable(key)
    }

    /// 设置共享变量。
    pub fn set_variable(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.context.set_variable(key, value);
    }

    /// 获取 ArtifactStore 引用（供 Executor 查询已有资产）。
    pub fn artifact_store(&self) -> &ArtifactStore {
        &self.context.artifact_store
    }

    /// 标记步骤开始（日志 + 计时起点）。
    pub fn begin_step(&mut self, step_index: u8) {
        self.context.last_updated_at = crate::domain::execution::now_rfc3339();
        eprintln!("[Runtime] step {step_index} started");
    }

    /// 报告步骤成功：添加资产 + 推进步骤。
    pub fn complete_step(&mut self, step_index: u8, artifact: Artifact) {
        self.context.artifact_store.push(artifact);
        self.step_results.push((
            step_index,
            StepOutcome::Success {
                artifact: self.context.artifact_store.all().last().unwrap().clone(),
            },
        ));
        self.context.advance_step();
    }

    /// 报告步骤失败：记录原因 + 推进步骤。
    pub fn fail_step(&mut self, step_index: u8, reason: String) {
        self.step_results
            .push((step_index, StepOutcome::Failed { reason }));
        self.context.advance_step();
    }

    /// 报告步骤跳过。
    pub fn skip_step(&mut self, step_index: u8) {
        self.step_results.push((step_index, StepOutcome::Skipped));
        self.context.advance_step();
    }

    /// 替换指定镜头的最新资产（Critic 重新生成时使用）。
    pub fn replace_artifact(&mut self, shot_index: u8, artifact: Artifact) {
        self.context
            .artifact_store
            .replace_latest(shot_index, artifact);
    }

    /// 完成执行，生成最终结果。
    pub fn finish(self) -> ExecutionResult {
        let mut completed_shots: Vec<u8> = Vec::new();
        let mut failed_shots: Vec<u8> = Vec::new();

        for (step_idx, outcome) in &self.step_results {
            match outcome {
                StepOutcome::Success { .. } | StepOutcome::Skipped => {
                    completed_shots.push(*step_idx);
                }
                StepOutcome::Failed { .. } => {
                    failed_shots.push(*step_idx);
                }
            }
        }

        let status = if failed_shots.is_empty() {
            ExecutionStatus::Completed
        } else if completed_shots.is_empty() {
            ExecutionStatus::Failed
        } else {
            ExecutionStatus::Partial
        };

        let duration_secs = self.start_instant.elapsed().as_secs_f64();

        ExecutionResult {
            plan_id: self.context.plan_id.clone(),
            status,
            completed_shots,
            failed_shots,
            output_artifacts: self.context.artifact_store.all().to_vec(),
            duration_secs,
        }
    }
}
