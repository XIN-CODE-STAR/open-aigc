#![allow(dead_code)]
use thiserror::Error;

use crate::{
    domain::agent::{AgentValidationError, PlanDraft, PlanRecord, PlanStepStatus},
    ports::persistence::PersistenceError,
};

#[derive(Debug, Error)]
pub enum PlanRepositoryError {
    #[error("agent plan {0} does not exist")]
    PlanNotFound(String),
    #[error(transparent)]
    Validation(#[from] AgentValidationError),
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

/// 计划仓储端口：持久化 Agent 执行计划。
///
/// 实现需保证：
/// - `create_plan` 插入计划并返回完整记录。
/// - `get_plan_by_conversation` 按会话查询最新计划。
/// - `update_plan_step` 更新单个步骤状态。
/// - `update_plan_status` 更新计划整体状态。
pub trait PlanRepository: Send {
    fn create_plan(&mut self, draft: PlanDraft) -> Result<PlanRecord, PlanRepositoryError>;

    fn get_plan_by_conversation(
        &mut self,
        conversation_id: &str,
    ) -> Result<Option<PlanRecord>, PlanRepositoryError>;

    fn update_plan_step(
        &mut self,
        plan_id: &str,
        step_index: u8,
        status: PlanStepStatus,
    ) -> Result<PlanRecord, PlanRepositoryError>;

    fn update_plan_status(
        &mut self,
        plan_id: &str,
        status: PlanStepStatus,
    ) -> Result<PlanRecord, PlanRepositoryError>;

    fn delete_plans_for_conversation(
        &mut self,
        conversation_id: &str,
    ) -> Result<(), PlanRepositoryError>;
}
