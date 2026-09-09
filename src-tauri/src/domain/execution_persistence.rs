#![allow(dead_code)]
//! Execution Persistence：执行持久化领域类型。
//!
//! 定义三张表的 Record + Draft 类型，用于 SQLite 持久化 Creative Runtime 执行状态。
//!
//! 三表关系：
//! - workflow_runs：一次执行实例（对应一个 ExecutionPlan）
//! - execution_steps：每步状态（pending / running / completed / failed / skipped）
//! - generation_submissions：远端生成任务（Facade.submit 产生）

use serde::{Deserialize, Serialize};

// ─── WorkflowRun ───

/// 一次执行实例的创建草稿。
#[derive(Debug, Clone)]
pub struct WorkflowRunDraft {
    pub workspace_id: String,
    pub plan_id: String,
    pub total_steps: u32,
}

/// 一次执行实例的持久化记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunRecord {
    pub id: String,
    pub workspace_id: String,
    pub plan_id: String,
    pub status: String,
    pub total_steps: u32,
    pub completed_steps: u32,
    pub failed_steps: u32,
    pub revision_iteration: u32,
    pub created_at: String,
    pub updated_at: String,
    pub metadata_json: String,
}

// ─── ExecutionStep ───

/// 一个执行步骤的创建草稿。
#[derive(Debug, Clone)]
pub struct ExecutionStepDraft {
    pub run_id: String,
    pub step_index: u32,
    pub kind: String,
    pub description: String,
}

/// 一个执行步骤的持久化记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionStepRecord {
    pub id: String,
    pub run_id: String,
    pub step_index: u32,
    pub kind: String,
    pub description: String,
    pub status: String,
    pub output_artifact_id: Option<String>,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub metadata_json: String,
}

// ─── GenerationSubmission ───

/// 一个远端生成任务的创建草稿。
#[derive(Debug, Clone)]
pub struct GenerationSubmissionDraft {
    pub step_id: String,
    pub step_index: u32,
    pub submission_id: String,
    pub provider_id: String,
    pub model: String,
}

/// 一个远端生成任务的持久化记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationSubmissionRecord {
    pub id: String,
    pub step_id: String,
    pub step_index: u32,
    pub submission_id: String,
    pub provider_id: String,
    pub model: String,
    pub status: String,
    pub asset_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
