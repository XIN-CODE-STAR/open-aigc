use std::path::PathBuf;
use std::sync::Mutex;

use crate::{
    application::error::AppError,
    domain::workflow::{
        WorkflowConfig, WorkflowError, WorkflowEvent, WorkflowEventType, WorkflowStage,
        WorkflowState,
    },
};

/// 创作工作流服务：管理完整的生成→评价→反馈→修改闭环。
///
/// 核心职责：
/// 1. 启动新的创作工作流
/// 2. 推进工作流阶段
/// 3. 暂停/恢复工作流（等待用户反馈）
/// 4. 记录工作流事件
/// 5. 查询工作流状态
pub struct WorkflowService {
    /// 内存中的工作流状态（持久化到 SQLite 的 workflow_events 表用于恢复）
    workflows: Mutex<Vec<WorkflowState>>,
    database_path: PathBuf,
}

impl WorkflowService {
    pub fn new(database_path: PathBuf) -> Self {
        Self {
            workflows: Mutex::new(Vec::new()),
            database_path,
        }
    }

    /// 启动新的创作工作流。
    pub fn start_workflow(
        &self,
        project_id: String,
        asset_id: String,
        shot_id: Option<String>,
        config: WorkflowConfig,
    ) -> Result<WorkflowState, AppError> {
        let now = crate::adapters::sqlite::now_rfc3339()?;
        let id = uuid::Uuid::new_v4().to_string();
        let max_iterations = config.max_iterations;

        let state = WorkflowState {
            id: id.clone(),
            project_id,
            asset_id,
            shot_id,
            current_stage: WorkflowStage::Generating,
            iteration: 0,
            max_iterations,
            current_review_id: None,
            current_plan_id: None,
            events: vec![WorkflowEvent {
                event_type: WorkflowEventType::StageEntered,
                stage: WorkflowStage::Generating,
                detail: Some("创作工作流已启动".into()),
                timestamp: now.clone(),
                review_report_id: None,
                edit_request_id: None,
                edit_plan_id: None,
            }],
            is_paused: false,
            error: None,
            created_at: now.clone(),
            updated_at: now,
        };

        let mut workflows = self
            .workflows
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        workflows.push(state.clone());
        Ok(state)
    }

    /// 推进工作流到下一阶段。
    pub fn advance_stage(
        &self,
        workflow_id: &str,
        next_stage: WorkflowStage,
        detail: Option<String>,
    ) -> Result<WorkflowState, AppError> {
        let now = crate::adapters::sqlite::now_rfc3339()?;
        let mut workflows = self
            .workflows
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let state = workflows
            .iter_mut()
            .find(|w| w.id == workflow_id)
            .ok_or_else(|| {
                AppError::new(
                    "workflow not found",
                    std::io::Error::other(format!("workflow {workflow_id} not found")),
                )
            })?;

        if state.current_stage.is_terminal() {
            return Err(AppError::new(
                "workflow already terminal",
                WorkflowError::AlreadyTerminal {
                    stage: state.current_stage,
                },
            ));
        }

        state.current_stage = next_stage;
        state.updated_at = now.clone();
        state.events.push(WorkflowEvent {
            event_type: if next_stage.is_terminal() && next_stage == WorkflowStage::Completed {
                WorkflowEventType::WorkflowCompleted
            } else if next_stage == WorkflowStage::Failed {
                WorkflowEventType::WorkflowFailed
            } else {
                WorkflowEventType::StageEntered
            },
            stage: next_stage,
            detail,
            timestamp: now,
            review_report_id: None,
            edit_request_id: None,
            edit_plan_id: None,
        });

        Ok(state.clone())
    }

    /// 暂停工作流（等待用户反馈）。
    pub fn pause_for_feedback(
        &self,
        workflow_id: &str,
        review_report_id: Option<String>,
    ) -> Result<WorkflowState, AppError> {
        let now = crate::adapters::sqlite::now_rfc3339()?;
        let mut workflows = self
            .workflows
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let state = workflows
            .iter_mut()
            .find(|w| w.id == workflow_id)
            .ok_or_else(|| {
                AppError::new(
                    "workflow not found",
                    std::io::Error::other(format!("workflow {workflow_id} not found")),
                )
            })?;

        state.current_stage = WorkflowStage::WaitingFeedback;
        state.is_paused = true;
        state.current_review_id = review_report_id;
        state.updated_at = now.clone();
        state.events.push(WorkflowEvent {
            event_type: WorkflowEventType::AutoEvaluationCompleted,
            stage: WorkflowStage::WaitingFeedback,
            detail: Some("自动评价完成，等待用户反馈".into()),
            timestamp: now,
            review_report_id: state.current_review_id.clone(),
            edit_request_id: None,
            edit_plan_id: None,
        });

        Ok(state.clone())
    }

    /// 用户提交反馈后恢复工作流。
    pub fn resume_with_feedback(
        &self,
        workflow_id: &str,
        edit_request_id: String,
    ) -> Result<WorkflowState, AppError> {
        let now = crate::adapters::sqlite::now_rfc3339()?;
        let mut workflows = self
            .workflows
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let state = workflows
            .iter_mut()
            .find(|w| w.id == workflow_id)
            .ok_or_else(|| {
                AppError::new(
                    "workflow not found",
                    std::io::Error::other(format!("workflow {workflow_id} not found")),
                )
            })?;

        if state.current_stage != WorkflowStage::WaitingFeedback {
            return Err(AppError::new(
                "not waiting for feedback",
                WorkflowError::NotWaitingFeedback,
            ));
        }

        state.current_stage = WorkflowStage::ParsingFeedback;
        state.is_paused = false;
        state.updated_at = now.clone();
        state.events.push(WorkflowEvent {
            event_type: WorkflowEventType::FeedbackReceived,
            stage: WorkflowStage::ParsingFeedback,
            detail: Some("用户提交反馈，开始解析".into()),
            timestamp: now,
            review_report_id: None,
            edit_request_id: Some(edit_request_id),
            edit_plan_id: None,
        });

        Ok(state.clone())
    }

    /// 标记修改计划已创建。
    pub fn mark_plan_created(
        &self,
        workflow_id: &str,
        edit_plan_id: String,
    ) -> Result<WorkflowState, AppError> {
        let now = crate::adapters::sqlite::now_rfc3339()?;
        let mut workflows = self
            .workflows
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let state = workflows
            .iter_mut()
            .find(|w| w.id == workflow_id)
            .ok_or_else(|| {
                AppError::new(
                    "workflow not found",
                    std::io::Error::other(format!("workflow {workflow_id} not found")),
                )
            })?;

        state.current_plan_id = Some(edit_plan_id.clone());
        state.current_stage = WorkflowStage::ApplyingModification;
        state.iteration += 1;
        state.updated_at = now.clone();
        state.events.push(WorkflowEvent {
            event_type: WorkflowEventType::ModificationPlanCreated,
            stage: WorkflowStage::ApplyingModification,
            detail: Some(format!("修改计划已创建，当前迭代: {}", state.iteration)),
            timestamp: now,
            review_report_id: None,
            edit_request_id: None,
            edit_plan_id: Some(edit_plan_id),
        });

        // 检查迭代限制
        if state.iteration >= state.max_iterations {
            state.events.push(WorkflowEvent {
                event_type: WorkflowEventType::WorkflowCompleted,
                stage: WorkflowStage::Completed,
                detail: Some("达到最大迭代次数，工作流完成".into()),
                timestamp: crate::adapters::sqlite::now_rfc3339()?,
                review_report_id: None,
                edit_request_id: None,
                edit_plan_id: None,
            });
            state.current_stage = WorkflowStage::Completed;
        }

        Ok(state.clone())
    }

    /// 标记重新生成已触发。
    pub fn mark_regeneration_triggered(
        &self,
        workflow_id: &str,
    ) -> Result<WorkflowState, AppError> {
        let now = crate::adapters::sqlite::now_rfc3339()?;
        let mut workflows = self
            .workflows
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let state = workflows
            .iter_mut()
            .find(|w| w.id == workflow_id)
            .ok_or_else(|| {
                AppError::new(
                    "workflow not found",
                    std::io::Error::other(format!("workflow {workflow_id} not found")),
                )
            })?;

        state.current_stage = WorkflowStage::Regenerating;
        state.updated_at = now.clone();
        state.events.push(WorkflowEvent {
            event_type: WorkflowEventType::RegenerationTriggered,
            stage: WorkflowStage::Regenerating,
            detail: Some("重新生成已触发".into()),
            timestamp: now,
            review_report_id: None,
            edit_request_id: None,
            edit_plan_id: None,
        });

        Ok(state.clone())
    }

    /// 标记工作流失败。
    pub fn mark_failed(
        &self,
        workflow_id: &str,
        error_msg: String,
    ) -> Result<WorkflowState, AppError> {
        let now = crate::adapters::sqlite::now_rfc3339()?;
        let mut workflows = self
            .workflows
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let state = workflows
            .iter_mut()
            .find(|w| w.id == workflow_id)
            .ok_or_else(|| {
                AppError::new(
                    "workflow not found",
                    std::io::Error::other(format!("workflow {workflow_id} not found")),
                )
            })?;

        state.current_stage = WorkflowStage::Failed;
        state.error = Some(error_msg.clone());
        state.updated_at = now.clone();
        state.events.push(WorkflowEvent {
            event_type: WorkflowEventType::WorkflowFailed,
            stage: WorkflowStage::Failed,
            detail: Some(error_msg),
            timestamp: now,
            review_report_id: None,
            edit_request_id: None,
            edit_plan_id: None,
        });

        Ok(state.clone())
    }

    /// 获取工作流状态。
    pub fn get_workflow(&self, workflow_id: &str) -> Result<Option<WorkflowState>, AppError> {
        let workflows = self
            .workflows
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        Ok(workflows.iter().find(|w| w.id == workflow_id).cloned())
    }

    /// 列出项目的所有工作流。
    pub fn list_workflows(&self, project_id: &str) -> Result<Vec<WorkflowState>, AppError> {
        let workflows = self
            .workflows
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        Ok(workflows
            .iter()
            .filter(|w| w.project_id == project_id)
            .cloned()
            .collect())
    }

    /// 列出等待反馈的工作流。
    pub fn list_waiting_workflows(&self) -> Result<Vec<WorkflowState>, AppError> {
        let workflows = self
            .workflows
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        Ok(workflows.iter().filter(|w| w.is_paused).cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, WorkflowService) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let service = WorkflowService::new(path);
        (dir, service)
    }

    #[test]
    fn starts_and_advances_workflow() {
        let (_dir, service) = setup();
        let state = service
            .start_workflow(
                "proj-1".into(),
                "asset-1".into(),
                None,
                WorkflowConfig::default(),
            )
            .unwrap();

        assert_eq!(state.current_stage, WorkflowStage::Generating);
        assert_eq!(state.iteration, 0);

        let state = service
            .advance_stage(&state.id, WorkflowStage::Evaluating, None)
            .unwrap();
        assert_eq!(state.current_stage, WorkflowStage::Evaluating);
    }

    #[test]
    fn pause_and_resume_workflow() {
        let (_dir, service) = setup();
        let state = service
            .start_workflow(
                "proj-1".into(),
                "asset-1".into(),
                None,
                WorkflowConfig::default(),
            )
            .unwrap();

        let state = service
            .pause_for_feedback(&state.id, Some("review-1".into()))
            .unwrap();
        assert!(state.is_paused);
        assert_eq!(state.current_stage, WorkflowStage::WaitingFeedback);

        let state = service
            .resume_with_feedback(&state.id, "req-1".into())
            .unwrap();
        assert!(!state.is_paused);
        assert_eq!(state.current_stage, WorkflowStage::ParsingFeedback);
    }

    #[test]
    fn workflow_completes_after_max_iterations() {
        let (_dir, service) = setup();
        let config = WorkflowConfig {
            max_iterations: 2,
            ..Default::default()
        };
        let state = service
            .start_workflow("proj-1".into(), "asset-1".into(), None, config)
            .unwrap();

        // Iteration 1
        let state = service
            .mark_plan_created(&state.id, "plan-1".into())
            .unwrap();
        assert_eq!(state.iteration, 1);
        assert_eq!(state.current_stage, WorkflowStage::ApplyingModification);

        // Iteration 2 → should complete
        let state = service
            .mark_plan_created(&state.id, "plan-2".into())
            .unwrap();
        assert_eq!(state.iteration, 2);
        assert_eq!(state.current_stage, WorkflowStage::Completed);
    }

    #[test]
    fn terminal_workflow_cannot_advance() {
        let (_dir, service) = setup();
        let state = service
            .start_workflow(
                "proj-1".into(),
                "asset-1".into(),
                None,
                WorkflowConfig::default(),
            )
            .unwrap();
        let state = service
            .advance_stage(&state.id, WorkflowStage::Completed, None)
            .unwrap();
        assert!(state.current_stage.is_terminal());

        let result = service.advance_stage(&state.id, WorkflowStage::Generating, None);
        assert!(result.is_err());
    }

    #[test]
    fn list_waiting_workflows() {
        let (_dir, service) = setup();
        let s1 = service
            .start_workflow(
                "proj-1".into(),
                "asset-1".into(),
                None,
                WorkflowConfig::default(),
            )
            .unwrap();
        let _s2 = service
            .start_workflow(
                "proj-1".into(),
                "asset-2".into(),
                None,
                WorkflowConfig::default(),
            )
            .unwrap();

        service.pause_for_feedback(&s1.id, None).unwrap();

        let waiting = service.list_waiting_workflows().unwrap();
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].id, s1.id);
    }
}
