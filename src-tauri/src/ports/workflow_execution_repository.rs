#![allow(dead_code)]
//! Workflow Execution Repository：执行持久化仓储端口。
//!
//! 定义 Creative Runtime 执行状态的持久化操作。
//! Application 层通过此 trait 与持久化层交互，不依赖具体实现（SQLite / 内存 / 其他）。
//!
//! 设计原则：
//! - Executor 通过 `ExecutionPersistenceEvent` 发射事件，Repository 消费事件
//! - 写入失败不阻断执行（"增强失败不影响主流程"）

use crate::domain::execution_persistence::{
    ExecutionStepDraft, ExecutionStepRecord, GenerationSubmissionDraft, GenerationSubmissionRecord,
    WorkflowRunDraft, WorkflowRunRecord,
};
use crate::ports::persistence::PersistenceError;

// ─── Error ───

#[derive(Debug, thiserror::Error)]
pub enum WorkflowExecutionRepositoryError {
    #[error("workflow run {0} not found")]
    RunNotFound(String),
    #[error("execution step {0} not found")]
    StepNotFound(String),
    #[error("generation submission {0} not found")]
    SubmissionNotFound(String),
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

// ─── ExecutionPersistenceEvent ───

/// 执行持久化事件（Executor 发射，Repository 消费）。
///
/// 轻量级内部事件，用于解耦 Executor 与具体持久化操作。
/// 未来可扩展为 WebSocket / Metrics / Event Bus 的入口。
#[derive(Debug, Clone)]
pub enum ExecutionPersistenceEvent {
    /// 执行开始。
    RunStarted { draft: WorkflowRunDraft },
    /// 执行完成（全部步骤结束）。
    RunCompleted {
        run_id: String,
        status: String,
        completed_steps: u32,
        failed_steps: u32,
    },
    /// 步骤开始。
    StepStarted { draft: ExecutionStepDraft },
    /// 步骤成功完成。
    StepCompleted {
        step_id: String,
        output_artifact_id: Option<String>,
    },
    /// 步骤失败。
    StepFailed { step_id: String, error: String },
    /// 步骤跳过（无对应 Skill）。
    StepSkipped { step_id: String },
    /// 远端生成任务已提交。
    SubmissionCreated { draft: GenerationSubmissionDraft },
    /// 远端生成任务状态更新。
    SubmissionUpdated {
        submission_id: String,
        status: String,
        asset_id: Option<String>,
    },
}

// ─── Repository Trait ───

/// 工作流执行仓储端口。
///
/// 支持 workflow_runs / execution_steps / generation_submissions 三表的 CRUD。
pub trait WorkflowExecutionRepository: Send {
    // ── WorkflowRun ──

    fn create_run(
        &mut self,
        draft: WorkflowRunDraft,
    ) -> Result<WorkflowRunRecord, WorkflowExecutionRepositoryError>;

    fn update_run_status(
        &mut self,
        id: &str,
        status: &str,
        completed_steps: u32,
        failed_steps: u32,
    ) -> Result<WorkflowRunRecord, WorkflowExecutionRepositoryError>;

    fn get_run(
        &mut self,
        id: &str,
    ) -> Result<Option<WorkflowRunRecord>, WorkflowExecutionRepositoryError>;

    fn list_runs_by_workspace(
        &mut self,
        workspace_id: &str,
    ) -> Result<Vec<WorkflowRunRecord>, WorkflowExecutionRepositoryError>;

    // ── ExecutionStep ──

    fn create_step(
        &mut self,
        draft: ExecutionStepDraft,
    ) -> Result<ExecutionStepRecord, WorkflowExecutionRepositoryError>;

    fn update_step_status(
        &mut self,
        id: &str,
        status: &str,
        output_artifact_id: Option<&str>,
        error: Option<&str>,
    ) -> Result<ExecutionStepRecord, WorkflowExecutionRepositoryError>;

    fn list_steps_by_run(
        &mut self,
        run_id: &str,
    ) -> Result<Vec<ExecutionStepRecord>, WorkflowExecutionRepositoryError>;

    // ── GenerationSubmission ──

    fn create_submission(
        &mut self,
        draft: GenerationSubmissionDraft,
    ) -> Result<GenerationSubmissionRecord, WorkflowExecutionRepositoryError>;

    fn update_submission_status(
        &mut self,
        id: &str,
        status: &str,
        asset_id: Option<&str>,
    ) -> Result<GenerationSubmissionRecord, WorkflowExecutionRepositoryError>;

    fn list_submissions_by_step(
        &mut self,
        step_id: &str,
    ) -> Result<Vec<GenerationSubmissionRecord>, WorkflowExecutionRepositoryError>;

    // ── Event Consumer ──

    /// 消费一个持久化事件。默认实现将事件分发到对应的 CRUD 方法。
    ///
    /// Executor 调用此方法而非直接调用 CRUD，保持解耦。
    fn consume_event(&mut self, event: &ExecutionPersistenceEvent) {
        match event {
            ExecutionPersistenceEvent::RunStarted { draft } => {
                let _ = self.create_run(draft.clone());
            }
            ExecutionPersistenceEvent::RunCompleted {
                run_id,
                status,
                completed_steps,
                failed_steps,
            } => {
                let _ = self.update_run_status(run_id, status, *completed_steps, *failed_steps);
            }
            ExecutionPersistenceEvent::StepStarted { draft } => {
                let _ = self.create_step(draft.clone());
            }
            ExecutionPersistenceEvent::StepCompleted {
                step_id,
                output_artifact_id,
            } => {
                let _ = self.update_step_status(
                    step_id,
                    "completed",
                    output_artifact_id.as_deref(),
                    None,
                );
            }
            ExecutionPersistenceEvent::StepFailed { step_id, error } => {
                let _ = self.update_step_status(step_id, "failed", None, Some(error));
            }
            ExecutionPersistenceEvent::StepSkipped { step_id } => {
                let _ = self.update_step_status(step_id, "skipped", None, None);
            }
            ExecutionPersistenceEvent::SubmissionCreated { draft } => {
                let _ = self.create_submission(draft.clone());
            }
            ExecutionPersistenceEvent::SubmissionUpdated {
                submission_id,
                status,
                asset_id,
            } => {
                let _ = self.update_submission_status(submission_id, status, asset_id.as_deref());
            }
        }
    }
}
