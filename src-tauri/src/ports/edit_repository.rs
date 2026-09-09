//! Edit Understanding Agent 持久化端口。
//!
//! 定义编辑请求和编辑计划的读写操作接口。

use thiserror::Error;

use crate::{
    domain::edit::{EditPlanRecord, EditPlanStatus, EditRequestRecord, EditRequestStatus},
    ports::persistence::PersistenceError,
};

#[derive(Debug, Error)]
pub enum EditRepositoryError {
    /// 指定的编辑请求不存在。
    #[error("edit request {0} does not exist")]
    RequestNotFound(String),
    /// 指定的编辑计划不存在。
    #[error("edit plan {0} does not exist")]
    PlanNotFound(String),
    /// 编辑请求已存在计划，不允许重复创建。
    #[error("edit request {request_id} already has plan {plan_id}")]
    DuplicatePlan { request_id: String, plan_id: String },
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

/// Edit Understanding Agent 持久化端口。
pub trait EditRepository: Send {
    // ═══════════════════════════════════════════════════════
    // 编辑请求
    // ═══════════════════════════════════════════════════════

    /// 插入新的编辑请求（状态为 received）。
    fn insert_request(
        &mut self,
        request: &EditRequestRecord,
    ) -> Result<EditRequestRecord, EditRepositoryError>;

    /// 按 ID 读取编辑请求。
    fn get_request(
        &mut self,
        request_id: &str,
    ) -> Result<Option<EditRequestRecord>, EditRepositoryError>;

    /// 列出项目的所有编辑请求（可选按 status 过滤）。
    fn list_requests_by_project(
        &mut self,
        project_id: &str,
        status: Option<EditRequestStatus>,
        limit: i64,
    ) -> Result<Vec<EditRequestRecord>, EditRepositoryError>;

    /// 更新编辑请求的状态。
    fn update_request_status(
        &mut self,
        request_id: &str,
        status: EditRequestStatus,
        resolved_at: Option<&str>,
    ) -> Result<EditRequestRecord, EditRepositoryError>;

    /// 将 LLM 解析的意图写入请求。
    fn update_request_intent(
        &mut self,
        request_id: &str,
        intent_json: &str,
        status: EditRequestStatus,
    ) -> Result<EditRequestRecord, EditRepositoryError>;

    // ═══════════════════════════════════════════════════════
    // 编辑计划
    // ═══════════════════════════════════════════════════════

    /// 插入新的编辑计划。
    fn insert_plan(&mut self, plan: &EditPlanRecord)
        -> Result<EditPlanRecord, EditRepositoryError>;

    /// 按 ID 读取编辑计划。
    fn get_plan(&mut self, plan_id: &str) -> Result<Option<EditPlanRecord>, EditRepositoryError>;

    /// 按编辑请求 ID 读取关联的计划。
    fn get_plan_by_request(
        &mut self,
        request_id: &str,
    ) -> Result<Option<EditPlanRecord>, EditRepositoryError>;

    /// 更新编辑计划的状态（含执行结果）。
    fn update_plan_status(
        &mut self,
        plan_id: &str,
        status: EditPlanStatus,
        execution_result_json: Option<&str>,
        executed_at: Option<&str>,
    ) -> Result<EditPlanRecord, EditRepositoryError>;
}
