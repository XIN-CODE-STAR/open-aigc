//! Edit Understanding Agent — SQLite 存储实现。
//!
//! 实现 EditRepository trait，操作 edit_requests / edit_plans 两张表。

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use super::now_rfc3339;
use crate::{
    domain::edit::{
        EditContextType, EditOperationType, EditPlanRecord, EditPlanStatus, EditRequestRecord,
        EditRequestStatus,
    },
    ports::{
        edit_repository::{EditRepository, EditRepositoryError},
        persistence::PersistenceError,
    },
};

pub struct SqliteEditRepository {
    connection: Connection,
}

impl SqliteEditRepository {
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        use crate::adapters::sqlite::database::open_database;
        let connection = open_database(database_path.as_ref())?;
        Ok(Self { connection })
    }
}

// ─────────────────────────────────────────────────────
// 编辑请求
// ─────────────────────────────────────────────────────

impl EditRepository for SqliteEditRepository {
    fn insert_request(
        &mut self,
        request: &EditRequestRecord,
    ) -> Result<EditRequestRecord, EditRepositoryError> {
        self.connection
            .execute(
                r#"
                INSERT INTO edit_requests (
                  id, project_id, run_id, feedback_text, context_type, context_ref_id,
                  source_review_id, intent_json, status, ambiguous_reason,
                  created_by, created_at, resolved_at
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
                "#,
                params![
                    request.id,
                    request.project_id,
                    request.run_id,
                    request.feedback_text,
                    request.context_type.as_str(),
                    request.context_ref_id,
                    request.source_review_id,
                    request.intent_json,
                    request.status.as_str(),
                    request.ambiguous_reason,
                    request.created_by,
                    request.created_at,
                    request.resolved_at,
                ],
            )
            .map_err(|e| PersistenceError::new("insert edit request", e))?;
        Ok(request.clone())
    }

    fn get_request(
        &mut self,
        request_id: &str,
    ) -> Result<Option<EditRequestRecord>, EditRepositoryError> {
        self.connection
            .query_row(
                "SELECT * FROM edit_requests WHERE id = ?1",
                params![request_id],
                map_edit_request,
            )
            .optional()
            .map_err(|e| PersistenceError::new("read edit request", e).into())
    }

    fn list_requests_by_project(
        &mut self,
        project_id: &str,
        status: Option<EditRequestStatus>,
        limit: i64,
    ) -> Result<Vec<EditRequestRecord>, EditRepositoryError> {
        let sql = if status.is_some() {
            "SELECT * FROM edit_requests WHERE project_id = ?1 AND status = ?2 ORDER BY created_at DESC LIMIT ?3"
        } else {
            "SELECT * FROM edit_requests WHERE project_id = ?1 ORDER BY created_at DESC LIMIT ?2"
        };

        let mut stmt = self
            .connection
            .prepare(sql)
            .map_err(|e| PersistenceError::new("prepare edit requests", e))?;

        let rows = if let Some(s) = status {
            stmt.query_map(params![project_id, s.as_str(), limit], map_edit_request)
        } else {
            stmt.query_map(params![project_id, limit], map_edit_request)
        }
        .map_err(|e| PersistenceError::new("query edit requests", e))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| PersistenceError::new("read edit request row", e))?);
        }
        Ok(records)
    }

    fn update_request_status(
        &mut self,
        request_id: &str,
        status: EditRequestStatus,
        resolved_at: Option<&str>,
    ) -> Result<EditRequestRecord, EditRepositoryError> {
        let affected = self
            .connection
            .execute(
                "UPDATE edit_requests SET status = ?1, resolved_at = ?2 WHERE id = ?3",
                params![status.as_str(), resolved_at, request_id],
            )
            .map_err(|e| PersistenceError::new("update edit request status", e))?;

        if affected == 0 {
            return Err(EditRepositoryError::RequestNotFound(request_id.into()));
        }

        self.connection
            .query_row(
                "SELECT * FROM edit_requests WHERE id = ?1",
                params![request_id],
                map_edit_request,
            )
            .map_err(|e| PersistenceError::new("read updated edit request", e).into())
    }

    fn update_request_intent(
        &mut self,
        request_id: &str,
        intent_json: &str,
        status: EditRequestStatus,
    ) -> Result<EditRequestRecord, EditRepositoryError> {
        let affected = self
            .connection
            .execute(
                "UPDATE edit_requests SET intent_json = ?1, status = ?2 WHERE id = ?3",
                params![intent_json, status.as_str(), request_id],
            )
            .map_err(|e| PersistenceError::new("update edit request intent", e))?;

        if affected == 0 {
            return Err(EditRepositoryError::RequestNotFound(request_id.into()));
        }

        self.connection
            .query_row(
                "SELECT * FROM edit_requests WHERE id = ?1",
                params![request_id],
                map_edit_request,
            )
            .map_err(|e| PersistenceError::new("read updated edit request", e).into())
    }

    // ─────────────────────────────────────────────────
    // 编辑计划
    // ─────────────────────────────────────────────────

    fn insert_plan(
        &mut self,
        plan: &EditPlanRecord,
    ) -> Result<EditPlanRecord, EditRepositoryError> {
        // 检查是否已存在计划
        let existing = self
            .connection
            .query_row(
                "SELECT id FROM edit_plans WHERE edit_request_id = ?1",
                params![plan.edit_request_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| PersistenceError::new("check duplicate plan", e))?;

        if let Some(existing_id) = existing {
            return Err(EditRepositoryError::DuplicatePlan {
                request_id: plan.edit_request_id.clone(),
                plan_id: existing_id,
            });
        }

        self.connection
            .execute(
                r#"
                INSERT INTO edit_plans (
                  id, edit_request_id, project_id,
                  plan_summary, operation_type, scope, targets_json,
                  prompt_patch_json, parameter_patch_json, reference_asset_patch_json,
                  requires_regeneration, requires_critic_rerun,
                  estimated_impact, risk_level,
                  status, execution_result_json,
                  created_by, created_at, executed_at
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)
                "#,
                params![
                    plan.id,
                    plan.edit_request_id,
                    plan.project_id,
                    plan.plan_summary,
                    plan.operation_type.as_str(),
                    plan.scope,
                    plan.targets_json,
                    plan.prompt_patch_json,
                    plan.parameter_patch_json,
                    plan.reference_asset_patch_json,
                    plan.requires_regeneration as i64,
                    plan.requires_critic_rerun as i64,
                    plan.estimated_impact,
                    plan.risk_level,
                    plan.status.as_str(),
                    plan.execution_result_json,
                    plan.created_by,
                    plan.created_at,
                    plan.executed_at,
                ],
            )
            .map_err(|e| PersistenceError::new("insert edit plan", e))?;

        Ok(plan.clone())
    }

    fn get_plan(&mut self, plan_id: &str) -> Result<Option<EditPlanRecord>, EditRepositoryError> {
        self.connection
            .query_row(
                "SELECT * FROM edit_plans WHERE id = ?1",
                params![plan_id],
                map_edit_plan,
            )
            .optional()
            .map_err(|e| PersistenceError::new("read edit plan", e).into())
    }

    fn get_plan_by_request(
        &mut self,
        request_id: &str,
    ) -> Result<Option<EditPlanRecord>, EditRepositoryError> {
        self.connection
            .query_row(
                "SELECT * FROM edit_plans WHERE edit_request_id = ?1",
                params![request_id],
                map_edit_plan,
            )
            .optional()
            .map_err(|e| PersistenceError::new("read edit plan by request", e).into())
    }

    fn update_plan_status(
        &mut self,
        plan_id: &str,
        status: EditPlanStatus,
        execution_result_json: Option<&str>,
        executed_at: Option<&str>,
    ) -> Result<EditPlanRecord, EditRepositoryError> {
        let affected = self
            .connection
            .execute(
                "UPDATE edit_plans SET status = ?1, execution_result_json = ?2, executed_at = ?3 WHERE id = ?4",
                params![status.as_str(), execution_result_json, executed_at, plan_id],
            )
            .map_err(|e| PersistenceError::new("update edit plan status", e))?;

        if affected == 0 {
            return Err(EditRepositoryError::PlanNotFound(plan_id.into()));
        }

        self.connection
            .query_row(
                "SELECT * FROM edit_plans WHERE id = ?1",
                params![plan_id],
                map_edit_plan,
            )
            .map_err(|e| PersistenceError::new("read updated edit plan", e).into())
    }
}

// ─────────────────────────────────────────────────────
// 行映射函数
// ─────────────────────────────────────────────────────

fn map_edit_request(row: &rusqlite::Row<'_>) -> rusqlite::Result<EditRequestRecord> {
    let ctx_str: String = row.get(4)?;
    let status_str: String = row.get(8)?;

    Ok(EditRequestRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        run_id: row.get(2)?,
        feedback_text: row.get(3)?,
        context_type: EditContextType::parse(&ctx_str).unwrap_or(EditContextType::GenerationResult),
        context_ref_id: row.get(5)?,
        source_review_id: row.get(6)?,
        intent_json: row.get(7)?,
        status: EditRequestStatus::parse(&status_str).unwrap_or(EditRequestStatus::Received),
        ambiguous_reason: row.get(9)?,
        created_by: row.get(10)?,
        created_at: row.get(11)?,
        resolved_at: row.get(12)?,
    })
}

fn map_edit_plan(row: &rusqlite::Row<'_>) -> rusqlite::Result<EditPlanRecord> {
    let op_str: String = row.get(4)?;
    let status_str: String = row.get(14)?;

    Ok(EditPlanRecord {
        id: row.get(0)?,
        edit_request_id: row.get(1)?,
        project_id: row.get(2)?,
        plan_summary: row.get(3)?,
        operation_type: EditOperationType::parse(&op_str).unwrap_or(EditOperationType::Regenerate),
        scope: row.get(5)?,
        targets_json: row.get(6)?,
        prompt_patch_json: row.get(7)?,
        parameter_patch_json: row.get(8)?,
        reference_asset_patch_json: row.get(9)?,
        requires_regeneration: row.get::<_, i64>(10)? != 0,
        requires_critic_rerun: row.get::<_, i64>(11)? != 0,
        estimated_impact: row.get(12)?,
        risk_level: row.get(13)?,
        status: EditPlanStatus::parse(&status_str).unwrap_or(EditPlanStatus::Draft),
        execution_result_json: row.get(15)?,
        created_by: row.get(16)?,
        created_at: row.get(17)?,
        executed_at: row.get(18)?,
    })
}

// ─────────────────────────────────────────────────────
// 测试
// ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::{
        adapters::sqlite::workspace_repository::SqliteWorkspaceRepository,
        domain::workspace::NewWorkspace, ports::workspace_repository::WorkspaceRepository,
    };

    fn seed() -> (tempfile::TempDir, SqliteEditRepository, String) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("workspace.sqlite3");
        let mut ws = SqliteWorkspaceRepository::open(&path).unwrap();
        ws.initialize(&NewWorkspace::try_new("测试", "老师").unwrap())
            .unwrap();
        drop(ws);
        let repo = SqliteEditRepository::open(&path).unwrap();
        let pid = "123e4567-e89b-42d3-a456-426614174000".to_owned();
        (dir, repo, pid)
    }

    #[test]
    fn inserts_and_reads_edit_request() {
        let (_dir, mut repo, pid) = seed();
        let now = now_rfc3339().unwrap();
        let request = EditRequestRecord {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: pid.clone(),
            run_id: None,
            feedback_text: "太假了，增加真实感".into(),
            context_type: EditContextType::GenerationResult,
            context_ref_id: None,
            source_review_id: None,
            intent_json: None,
            status: EditRequestStatus::Received,
            ambiguous_reason: None,
            created_by: "teacher-id".into(),
            created_at: now,
            resolved_at: None,
        };

        let inserted = repo.insert_request(&request).unwrap();
        assert_eq!(inserted.id, request.id);
        assert_eq!(inserted.status, EditRequestStatus::Received);

        let fetched = repo.get_request(&request.id).unwrap().unwrap();
        assert_eq!(fetched.feedback_text, "太假了，增加真实感");
    }

    #[test]
    fn creates_plan_and_reads_back() {
        let (_dir, mut repo, pid) = seed();
        let now = now_rfc3339().unwrap();
        let req_id = uuid::Uuid::new_v4().to_string();

        let request = EditRequestRecord {
            id: req_id.clone(),
            project_id: pid.clone(),
            run_id: None,
            feedback_text: "更高级一点".into(),
            context_type: EditContextType::GenerationResult,
            context_ref_id: None,
            source_review_id: None,
            intent_json: None,
            status: EditRequestStatus::Received,
            ambiguous_reason: None,
            created_by: "teacher-id".into(),
            created_at: now.clone(),
            resolved_at: None,
        };
        repo.insert_request(&request).unwrap();

        let plan = EditPlanRecord {
            id: uuid::Uuid::new_v4().to_string(),
            edit_request_id: req_id.clone(),
            project_id: pid.clone(),
            plan_summary: "提升空间尺度，增加广角镜头".into(),
            operation_type: EditOperationType::CompositionChange,
            scope: "whole".into(),
            targets_json: r#"[{"type":"visual_style","id":"vs-1","operation":"grand_scale"}]"#
                .into(),
            prompt_patch_json: Some(r#"{"add":["grand scale","wide angle"]}"#.into()),
            parameter_patch_json: None,
            reference_asset_patch_json: None,
            requires_regeneration: true,
            requires_critic_rerun: true,
            estimated_impact: Some("medium".into()),
            risk_level: Some("low".into()),
            status: EditPlanStatus::Ready,
            execution_result_json: None,
            created_by: "agent".into(),
            created_at: now,
            executed_at: None,
        };

        let inserted = repo.insert_plan(&plan).unwrap();
        assert_eq!(inserted.status, EditPlanStatus::Ready);

        let by_request = repo.get_plan_by_request(&req_id).unwrap().unwrap();
        assert_eq!(by_request.id, plan.id);
        assert_eq!(
            by_request.operation_type,
            EditOperationType::CompositionChange
        );
    }

    #[test]
    fn rejects_duplicate_plan_for_same_request() {
        let (_dir, mut repo, pid) = seed();
        let now = now_rfc3339().unwrap();
        let req_id = uuid::Uuid::new_v4().to_string();

        let request = EditRequestRecord {
            id: req_id.clone(),
            project_id: pid.clone(),
            run_id: None,
            feedback_text: "改一下".into(),
            context_type: EditContextType::GenerationResult,
            context_ref_id: None,
            source_review_id: None,
            intent_json: None,
            status: EditRequestStatus::Received,
            ambiguous_reason: None,
            created_by: "t".into(),
            created_at: now.clone(),
            resolved_at: None,
        };
        repo.insert_request(&request).unwrap();

        let plan = EditPlanRecord {
            id: uuid::Uuid::new_v4().to_string(),
            edit_request_id: req_id.clone(),
            project_id: pid.clone(),
            plan_summary: "summary".into(),
            operation_type: EditOperationType::Regenerate,
            scope: "whole".into(),
            targets_json: "[]".into(),
            prompt_patch_json: None,
            parameter_patch_json: None,
            reference_asset_patch_json: None,
            requires_regeneration: true,
            requires_critic_rerun: true,
            estimated_impact: None,
            risk_level: None,
            status: EditPlanStatus::Draft,
            execution_result_json: None,
            created_by: "agent".into(),
            created_at: now,
            executed_at: None,
        };
        repo.insert_plan(&plan).unwrap();

        // Duplicate
        let plan2 = EditPlanRecord {
            id: uuid::Uuid::new_v4().to_string(),
            ..plan.clone()
        };
        let err = repo.insert_plan(&plan2).unwrap_err();
        assert!(matches!(err, EditRepositoryError::DuplicatePlan { .. }));
    }
}
