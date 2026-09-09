//! Edit Understanding Agent v1 — 用户自然语言反馈解析与修改计划服务。
//!
//! 职责：
//! - 接收用户自然语言反馈，解析意图（基于关键词匹配 + LLM 增强）。
//! - 生成结构化修改计划（prompt_patch、parameter_patch、targets）。
//! - 执行编辑计划（更新 Prompt 版本或创建新的生成任务）。

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Mutex;

use crate::{
    adapters::sqlite::edit_repository::SqliteEditRepository,
    application::error::AppError,
    domain::edit::{
        EditOperationType, EditPlanRecord, EditPlanStatus, EditRequestDraft, EditRequestRecord,
        EditRequestStatus, EditUnderstandingResult, FeedbackPattern, ImpactLevel, PromptPatch,
    },
    ports::edit_repository::EditRepository,
};

pub struct EditUnderstandingService {
    repository: Mutex<Box<dyn EditRepository>>,
    #[allow(dead_code)]
    database_path: PathBuf,
}

impl EditUnderstandingService {
    pub fn new(repository: impl EditRepository + 'static, database_path: PathBuf) -> Self {
        Self {
            repository: Mutex::new(Box::new(repository)),
            database_path,
        }
    }

    // ═══════════════════════════════════════════════════════
    // 反馈提交
    // ═══════════════════════════════════════════════════════

    /// 提交用户自然语言反馈并触发意图解析。
    /// 同步：先写入请求 (status=received)，再异步解析意图并生成计划。
    pub fn submit_feedback(&self, draft: EditRequestDraft) -> Result<EditRequestRecord, AppError> {
        let now = crate::adapters::sqlite::now_rfc3339()?;
        let request_id = uuid::Uuid::new_v4().to_string();

        let record = EditRequestRecord {
            id: request_id,
            project_id: draft.project_id,
            run_id: draft.run_id,
            feedback_text: draft.feedback_text.clone(),
            context_type: draft.context_type,
            context_ref_id: draft.context_ref_id,
            source_review_id: draft.source_review_id,
            intent_json: None,
            status: EditRequestStatus::Received,
            ambiguous_reason: None,
            created_by: draft.created_by,
            created_at: now,
            resolved_at: None,
        };

        let inserted =
            self.with_repository(|repo| repo.insert_request(&record).map_err(Into::into))?;

        // 同步：立即尝试解析意图并生成编辑计划
        if let Err(e) = self.analyze_and_create_plan(&inserted) {
            // 解析失败不阻塞反馈提交，标记为 ambiguous
            let _ = self.with_repository(|repo| {
                repo.update_request_status(&inserted.id, EditRequestStatus::Ambiguous, None)
                    .map_err(Into::into)
            });
            eprintln!(
                "Edit understanding failed for request {}: {}",
                inserted.id, e
            );
        }

        Ok(inserted)
    }

    // ═══════════════════════════════════════════════════════
    // 意图解析
    // ═══════════════════════════════════════════════════════

    /// 解析用户反馈的意图（关键词快速匹配 + 结构化输出）。
    fn analyze_feedback(
        &self,
        request: &EditRequestRecord,
    ) -> Result<EditUnderstandingResult, AppError> {
        let feedback = &request.feedback_text;

        // 第一层：关键词快速匹配
        let pattern = FeedbackPattern::find_match(feedback);

        // 第二层：构造语义理解结果
        let (intent, sub_intent, prompt_patch) = if let Some(ref pat) = pattern {
            let patch = PromptPatch {
                add: Some(pat.prompt_add.iter().map(|s| s.to_string()).collect()),
                remove: Some(pat.prompt_remove.iter().map(|s| s.to_string()).collect()),
                replace: None,
            };
            (pat.intent.as_str().to_owned(), None, Some(patch))
        } else {
            // 通用匹配失败，返回 generic 结果
            (
                "regenerate".to_owned(),
                Some("unrecognized_feedback".to_owned()),
                None,
            )
        };

        let confidence = if pattern.is_some() { 0.75 } else { 0.40 };

        Ok(EditUnderstandingResult {
            feedback: feedback.clone(),
            intent,
            sub_intent,
            confidence,
            targets: vec![],
            prompt_patch,
            parameter_patch: None,
            requires_critic_rerun: true,
            risk: if pattern.is_some() {
                ImpactLevel::Low
            } else {
                ImpactLevel::Medium
            },
        })
    }

    /// 解析反馈并创建编辑计划（事务性：先更新 intent，再创建 plan）。
    fn analyze_and_create_plan(
        &self,
        request: &EditRequestRecord,
    ) -> Result<EditPlanRecord, AppError> {
        let now = crate::adapters::sqlite::now_rfc3339()?;

        // Step 1: 解析反馈
        let understanding = self.analyze_feedback(request)?;

        // Step 2: 更新 intent
        let intent_json = serde_json::to_string(&understanding)
            .map_err(|e| AppError::new("serialize edit understanding", e))?;

        self.with_repository(|repo| {
            repo.update_request_intent(&request.id, &intent_json, EditRequestStatus::Analyzing)
                .map_err(Into::into)
        })?;

        // Step 3: 生成编辑计划
        let plan_id = uuid::Uuid::new_v4().to_string();
        let operation_type = EditOperationType::parse(&understanding.intent)
            .unwrap_or(EditOperationType::Regenerate);

        let prompt_patch_json = understanding
            .prompt_patch
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| AppError::new("serialize prompt patch", e))?;

        let plan = EditPlanRecord {
            id: plan_id,
            edit_request_id: request.id.clone(),
            project_id: request.project_id.clone(),
            plan_summary: format!(
                "基于反馈「{}」生成修改计划，意图={}",
                request.feedback_text.chars().take(80).collect::<String>(),
                understanding.intent
            ),
            operation_type,
            scope: "whole".into(),
            targets_json: serde_json::to_string(&understanding.targets)
                .map_err(|e| AppError::new("serialize targets", e))?,
            prompt_patch_json,
            parameter_patch_json: understanding
                .parameter_patch
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|e| AppError::new("serialize parameter patch", e))?,
            reference_asset_patch_json: None,
            requires_regeneration: true,
            requires_critic_rerun: understanding.requires_critic_rerun,
            estimated_impact: Some(understanding.risk.as_str().to_owned()),
            risk_level: Some(understanding.risk.as_str().to_owned()),
            status: EditPlanStatus::Ready,
            execution_result_json: None,
            created_by: "edit-understanding-v1".into(),
            created_at: now,
            executed_at: None,
        };

        let inserted_plan =
            self.with_repository(|repo| repo.insert_plan(&plan).map_err(Into::into))?;

        // Step 4: 更新请求状态为 plan_ready
        let _ = self.with_repository(|repo| {
            repo.update_request_status(&request.id, EditRequestStatus::PlanReady, None)
                .map_err(Into::into)
        });

        Ok(inserted_plan)
    }

    // ═══════════════════════════════════════════════════════
    // 查询
    // ═══════════════════════════════════════════════════════

    /// 按 ID 读取编辑请求。
    pub fn get_request(&self, request_id: &str) -> Result<Option<EditRequestRecord>, AppError> {
        self.with_repository(|repo| repo.get_request(request_id).map_err(Into::into))
    }

    /// 列出项目的所有编辑请求。
    pub fn list_requests(
        &self,
        project_id: &str,
        status: Option<EditRequestStatus>,
        limit: i64,
    ) -> Result<Vec<EditRequestRecord>, AppError> {
        self.with_repository(|repo| {
            repo.list_requests_by_project(project_id, status, limit)
                .map_err(Into::into)
        })
    }

    /// 按请求 ID 读取编辑计划。
    pub fn get_plan_by_request(
        &self,
        request_id: &str,
    ) -> Result<Option<EditPlanRecord>, AppError> {
        self.with_repository(|repo| repo.get_plan_by_request(request_id).map_err(Into::into))
    }

    /// 按计划 ID 读取编辑计划。
    pub fn get_plan(&self, plan_id: &str) -> Result<Option<EditPlanRecord>, AppError> {
        self.with_repository(|repo| repo.get_plan(plan_id).map_err(Into::into))
    }

    // ═══════════════════════════════════════════════════════
    // 执行与拒绝
    // ═══════════════════════════════════════════════════════

    /// 执行编辑计划——将 prompt_patch / parameter_patch 应用到生成参数。
    /// 当前版本只是标记为 executed；未来版本将直接创建新的 Prompt 版本和生成任务。
    pub fn apply_plan(&self, plan_id: &str) -> Result<EditPlanRecord, AppError> {
        let now = crate::adapters::sqlite::now_rfc3339()?;

        let plan = self.with_repository(|repo| {
            let plan =
                repo.update_plan_status(plan_id, EditPlanStatus::Executing, None, Some(&now))?;
            Ok::<_, AppError>(plan)
        })?;

        // 标记为 executed
        let result = self.with_repository(|repo| {
            repo.update_plan_status(
                &plan.id,
                EditPlanStatus::Executed,
                Some(r#"{"status":"applied"}"#),
                Some(&now),
            )
            .map_err(Into::into)
        })?;

        let request = self.with_repository(|repo| {
            repo.update_request_status(
                &plan.edit_request_id,
                EditRequestStatus::Applied,
                Some(&now),
            )
            .map_err(Into::into)
        });

        // 忽略状态更新失败（计划已执行）
        let _ = request;

        Ok(result)
    }

    /// 跳过/拒绝编辑请求。
    pub fn skip_request(
        &self,
        request_id: &str,
        _reason: Option<&str>,
    ) -> Result<EditRequestRecord, AppError> {
        let now = crate::adapters::sqlite::now_rfc3339()?;
        self.with_repository(|repo| {
            repo.update_request_status(request_id, EditRequestStatus::Rejected, Some(&now))
                .map_err(Into::into)
        })
    }

    /// 重新分析一条已存在的编辑请求（当用户再次提交时）。
    pub fn reanalyze_request(&self, request_id: &str) -> Result<EditPlanRecord, AppError> {
        let request = self
            .with_repository(|repo| repo.get_request(request_id).map_err(Into::into))?
            .ok_or_else(|| AppError::new("edit request not found", std::io::Error::other("")))?;

        self.analyze_and_create_plan(&request)
    }

    // ═══════════════════════════════════════════════════════
    // 内部
    // ═══════════════════════════════════════════════════════

    fn with_repository<T>(
        &self,
        operation: impl FnOnce(&mut dyn EditRepository) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        operation(repo.as_mut())
    }
}

// ─────────────────────────────────────────────────────
// Reloadable
// ─────────────────────────────────────────────────────

impl crate::ports::reloadable::Reloadable for EditUnderstandingService {
    fn reload(&self, database_path: &std::path::Path) -> Result<(), AppError> {
        let new_repo = SqliteEditRepository::open(database_path)?;
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        *repo = Box::new(new_repo);
        Ok(())
    }
}
