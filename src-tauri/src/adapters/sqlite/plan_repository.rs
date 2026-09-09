use std::path::Path;

use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::{
    domain::agent::{PlanDraft, PlanRecord, PlanStep, PlanStepStatus},
    ports::{
        persistence::PersistenceError,
        plan_repository::{PlanRepository, PlanRepositoryError},
    },
};

pub struct SqlitePlanRepository {
    connection: rusqlite::Connection,
}

impl SqlitePlanRepository {
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        use crate::adapters::sqlite::database::open_database;
        let connection = open_database(database_path.as_ref())?;
        Ok(Self { connection })
    }
}

impl PlanRepository for SqlitePlanRepository {
    fn create_plan(&mut self, draft: PlanDraft) -> Result<PlanRecord, PlanRepositoryError> {
        let id = Uuid::new_v4().to_string();
        let now = now_rfc3339()?;
        let steps_json = serde_json::to_string(&draft.steps)
            .map_err(|e| PersistenceError::new("serialize plan steps", e))?;

        self.connection
            .execute(
                r#"
                INSERT INTO agent_plans (id, conversation_id, goal, steps_json, status, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?5)
                "#,
                params![id, draft.conversation_id, draft.goal, steps_json, now],
            )
            .map_err(|e| PersistenceError::new("insert agent plan", e))?;

        Ok(PlanRecord {
            id,
            conversation_id: draft.conversation_id,
            goal: draft.goal,
            steps: draft.steps,
            status: PlanStepStatus::Pending,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    fn get_plan_by_conversation(
        &mut self,
        conversation_id: &str,
    ) -> Result<Option<PlanRecord>, PlanRepositoryError> {
        let record = self
            .connection
            .query_row(
                r#"
                SELECT id, conversation_id, goal, steps_json, status, created_at, updated_at
                FROM agent_plans
                WHERE conversation_id = ?1
                ORDER BY created_at DESC
                LIMIT 1
                "#,
                params![conversation_id],
                map_plan,
            )
            .optional()
            .map_err(|e| PersistenceError::new("read agent plan", e))?;
        Ok(record)
    }

    fn update_plan_step(
        &mut self,
        plan_id: &str,
        step_index: u8,
        status: PlanStepStatus,
    ) -> Result<PlanRecord, PlanRepositoryError> {
        // 读取当前计划
        let mut plan = self
            .connection
            .query_row(
                r#"
                SELECT id, conversation_id, goal, steps_json, status, created_at, updated_at
                FROM agent_plans WHERE id = ?1
                "#,
                params![plan_id],
                map_plan,
            )
            .map_err(|e| PersistenceError::new("read agent plan for step update", e))?;

        // 更新步骤状态
        let mut steps: Vec<PlanStep> = plan.steps;
        if let Some(step) = steps.iter_mut().find(|s| s.index == step_index) {
            step.status = status;
        }
        let steps_json = serde_json::to_string(&steps)
            .map_err(|e| PersistenceError::new("serialize plan steps", e))?;

        // 计算整体状态
        let overall = compute_overall_status(&steps);
        let now = now_rfc3339()?;

        self.connection
            .execute(
                "UPDATE agent_plans SET steps_json = ?1, status = ?2, updated_at = ?3 WHERE id = ?4",
                params![steps_json, overall.as_str(), now, plan_id],
            )
            .map_err(|e| PersistenceError::new("update agent plan step", e))?;

        plan.steps = steps;
        plan.status = overall;
        plan.updated_at = now;
        Ok(plan)
    }

    fn update_plan_status(
        &mut self,
        plan_id: &str,
        status: PlanStepStatus,
    ) -> Result<PlanRecord, PlanRepositoryError> {
        let now = now_rfc3339()?;
        self.connection
            .execute(
                "UPDATE agent_plans SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status.as_str(), now, plan_id],
            )
            .map_err(|e| PersistenceError::new("update agent plan status", e))?;

        let plan = self
            .connection
            .query_row(
                r#"
                SELECT id, conversation_id, goal, steps_json, status, created_at, updated_at
                FROM agent_plans WHERE id = ?1
                "#,
                params![plan_id],
                map_plan,
            )
            .map_err(|e| PersistenceError::new("read agent plan after status update", e))?;
        Ok(plan)
    }

    fn delete_plans_for_conversation(
        &mut self,
        conversation_id: &str,
    ) -> Result<(), PlanRepositoryError> {
        self.connection
            .execute(
                "DELETE FROM agent_plans WHERE conversation_id = ?1",
                params![conversation_id],
            )
            .map_err(|e| PersistenceError::new("delete agent plans", e))?;
        Ok(())
    }
}

fn map_plan(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlanRecord> {
    let id: String = row.get(0)?;
    let conversation_id: String = row.get(1)?;
    let goal: String = row.get(2)?;
    let steps_json: String = row.get(3)?;
    let status_str: String = row.get(4)?;
    let created_at: String = row.get(5)?;
    let updated_at: String = row.get(6)?;

    let steps: Vec<PlanStep> = serde_json::from_str(&steps_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let status = PlanStepStatus::parse(&status_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
    })?;

    Ok(PlanRecord {
        id,
        conversation_id,
        goal,
        steps,
        status,
        created_at,
        updated_at,
    })
}

fn compute_overall_status(steps: &[PlanStep]) -> PlanStepStatus {
    let all_completed = steps
        .iter()
        .all(|s| s.status == PlanStepStatus::Completed || s.status == PlanStepStatus::Skipped);
    let any_failed = steps.iter().any(|s| s.status == PlanStepStatus::Failed);
    let any_in_progress = steps.iter().any(|s| s.status == PlanStepStatus::InProgress);

    if all_completed {
        PlanStepStatus::Completed
    } else if any_failed {
        PlanStepStatus::Failed
    } else if any_in_progress {
        PlanStepStatus::InProgress
    } else {
        PlanStepStatus::Pending
    }
}

fn now_rfc3339() -> Result<String, PersistenceError> {
    let now = time::OffsetDateTime::now_utc();
    now.format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| PersistenceError::new("format timestamp", e))
}
