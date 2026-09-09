//! AI Critic Agent 评价系统 — SQLite 存储实现。
//!
//! 实现 ReviewRepository trait，操作 review_reports / review_dimensions /
//! asset_versions / asset_licenses / content_guard_reports 六张表。

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use super::now_rfc3339;
use crate::{
    domain::review::{
        AssetLicenseRecord, AssetVersionRecord, AssetVersionStatus, CommercialUseStatus,
        ContentGuardReportRecord, ContentGuardStatus, ContentRiskLevel, LicenseSourceType,
        ReviewDecision, ReviewDimensionLayer, ReviewDimensionRecord, ReviewReportRecord,
        ReviewerType,
    },
    ports::{
        persistence::PersistenceError,
        review_repository::{ReviewRepository, ReviewRepositoryError},
    },
};

pub struct SqliteReviewRepository {
    connection: Connection,
}

impl SqliteReviewRepository {
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        use crate::adapters::sqlite::database::open_database;
        let connection = open_database(database_path.as_ref())?;
        Ok(Self { connection })
    }
}

// ─────────────────────────────────────────────────────
// 评价报告
// ─────────────────────────────────────────────────────

impl ReviewRepository for SqliteReviewRepository {
    fn insert_report(
        &mut self,
        report: &ReviewReportRecord,
    ) -> Result<ReviewReportRecord, ReviewRepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::new("begin insert review report", e))?;

        transaction
            .execute(
                r#"
                INSERT INTO review_reports (
                  id, project_id, run_id, shot_id, asset_id, generation_attempt_id,
                  reviewer_type, reviewer_agent_version, reviewer_provider,
                  requirement_scores_json, visual_scores_json, content_scores_json,
                  commercial_scores_json, technical_scores_json,
                  overall_score, weighted_score, issues_json, decision, confidence,
                  source_task_id, review_version, created_at
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22)
                "#,
                params![
                    report.id,
                    report.project_id,
                    report.run_id,
                    report.shot_id,
                    report.asset_id,
                    report.generation_attempt_id,
                    report.reviewer_type.as_str(),
                    report.reviewer_agent_version,
                    report.reviewer_provider,
                    report.requirement_scores_json,
                    report.visual_scores_json,
                    report.content_scores_json,
                    report.commercial_scores_json,
                    report.technical_scores_json,
                    report.overall_score,
                    report.weighted_score,
                    report.issues_json,
                    report.decision.as_str(),
                    report.confidence,
                    report.source_task_id,
                    report.review_version,
                    report.created_at,
                ],
            )
            .map_err(|e| PersistenceError::new("insert review report", e))?;

        transaction
            .commit()
            .map_err(|e| PersistenceError::new("commit review report", e))?;

        Ok(report.clone())
    }

    fn get_report(
        &mut self,
        report_id: &str,
    ) -> Result<Option<ReviewReportRecord>, ReviewRepositoryError> {
        self.connection
            .query_row(
                "SELECT * FROM review_reports WHERE id = ?1",
                params![report_id],
                map_review_report,
            )
            .optional()
            .map_err(|e| PersistenceError::new("read review report", e).into())
    }

    fn list_reports_by_project(
        &mut self,
        project_id: &str,
        decision: Option<ReviewDecision>,
        limit: i64,
    ) -> Result<Vec<ReviewReportRecord>, ReviewRepositoryError> {
        let mut sql = String::from("SELECT * FROM review_reports WHERE project_id = ?1");
        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> =
            vec![Box::new(project_id.to_owned())];

        if let Some(d) = decision {
            sql.push_str(" AND decision = ?2");
            params_vec.push(Box::new(d.as_str().to_owned()));
            sql.push_str(&format!(
                " ORDER BY created_at DESC LIMIT ?{}",
                params_vec.len() + 1
            ));
            params_vec.push(Box::new(limit));
        } else {
            sql.push_str(" ORDER BY created_at DESC LIMIT ?2");
            params_vec.push(Box::new(limit));
        }

        let mut stmt = self
            .connection
            .prepare(&sql)
            .map_err(|e| PersistenceError::new("prepare review list", e))?;

        let rows = stmt
            .query_map(
                rusqlite::params_from_iter(params_vec.iter().map(|b| b.as_ref())),
                map_review_report,
            )
            .map_err(|e| PersistenceError::new("query review list", e))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| PersistenceError::new("read review row", e))?);
        }
        Ok(records)
    }

    fn list_reports_by_shot(
        &mut self,
        shot_id: &str,
    ) -> Result<Vec<ReviewReportRecord>, ReviewRepositoryError> {
        let mut stmt = self
            .connection
            .prepare("SELECT * FROM review_reports WHERE shot_id = ?1 ORDER BY created_at DESC")
            .map_err(|e| PersistenceError::new("prepare review by shot", e))?;
        let rows = stmt
            .query_map(params![shot_id], map_review_report)
            .map_err(|e| PersistenceError::new("query review by shot", e))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| PersistenceError::new("read review by shot", e))?);
        }
        Ok(records)
    }

    fn list_reports_by_asset(
        &mut self,
        asset_id: &str,
    ) -> Result<Vec<ReviewReportRecord>, ReviewRepositoryError> {
        let mut stmt = self
            .connection
            .prepare("SELECT * FROM review_reports WHERE asset_id = ?1 ORDER BY created_at DESC")
            .map_err(|e| PersistenceError::new("prepare review by asset", e))?;
        let rows = stmt
            .query_map(params![asset_id], map_review_report)
            .map_err(|e| PersistenceError::new("query review by asset", e))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| PersistenceError::new("read review by asset", e))?);
        }
        Ok(records)
    }

    fn list_reports_by_attempt(
        &mut self,
        attempt_id: &str,
    ) -> Result<Vec<ReviewReportRecord>, ReviewRepositoryError> {
        let mut stmt = self
            .connection
            .prepare(
                "SELECT * FROM review_reports WHERE generation_attempt_id = ?1 ORDER BY created_at DESC",
            )
            .map_err(|e| PersistenceError::new("prepare review by attempt", e))?;
        let rows = stmt
            .query_map(params![attempt_id], map_review_report)
            .map_err(|e| PersistenceError::new("query review by attempt", e))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| PersistenceError::new("read review by attempt", e))?);
        }
        Ok(records)
    }

    // ─────────────────────────────────────────────────
    // 评价维度
    // ─────────────────────────────────────────────────

    fn insert_dimensions(
        &mut self,
        dimensions: &[ReviewDimensionRecord],
    ) -> Result<(), ReviewRepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::new("begin insert dimensions", e))?;

        for dim in dimensions {
            transaction
                .execute(
                    r#"
                    INSERT INTO review_dimensions (
                      id, review_id, dimension_layer, dimension_name,
                      score, weight, confidence, reasoning, reference_context_json, created_at
                    ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
                    "#,
                    params![
                        dim.id,
                        dim.review_id,
                        dim.dimension_layer.as_str(),
                        dim.dimension_name,
                        dim.score,
                        dim.weight,
                        dim.confidence,
                        dim.reasoning,
                        dim.reference_context_json,
                        dim.created_at,
                    ],
                )
                .map_err(|e| PersistenceError::new("insert review dimension", e))?;
        }

        transaction
            .commit()
            .map_err(|e| PersistenceError::new("commit dimensions", e))?;
        Ok(())
    }

    fn list_dimensions_by_report(
        &mut self,
        review_id: &str,
    ) -> Result<Vec<ReviewDimensionRecord>, ReviewRepositoryError> {
        let mut stmt = self
            .connection
            .prepare(
                "SELECT * FROM review_dimensions WHERE review_id = ?1 ORDER BY dimension_layer, dimension_name",
            )
            .map_err(|e| PersistenceError::new("prepare dimensions list", e))?;
        let rows = stmt
            .query_map(params![review_id], map_dimension_record)
            .map_err(|e| PersistenceError::new("query dimensions list", e))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| PersistenceError::new("read dimension row", e))?);
        }
        Ok(records)
    }

    // ─────────────────────────────────────────────────
    // 资产版本
    // ─────────────────────────────────────────────────

    fn insert_asset_version(
        &mut self,
        version: &AssetVersionRecord,
    ) -> Result<AssetVersionRecord, ReviewRepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::new("begin insert asset version", e))?;

        transaction
            .execute(
                r#"
                INSERT INTO asset_versions (
                  id, asset_id, version, storage_key, mime_type, size_bytes,
                  hash, width, height, duration_seconds,
                  source_type, source_task_id, source_attempt_id,
                  status, status_reason, review_id, created_by, created_at, updated_at
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)
                "#,
                params![
                    version.id,
                    version.asset_id,
                    version.version,
                    version.storage_key,
                    version.mime_type,
                    version.size_bytes,
                    version.hash,
                    version.width,
                    version.height,
                    version.duration_seconds,
                    version.source_type,
                    version.source_task_id,
                    version.source_attempt_id,
                    version.status.as_str(),
                    version.status_reason,
                    version.review_id,
                    version.created_by,
                    version.created_at,
                    version.updated_at,
                ],
            )
            .map_err(|e| PersistenceError::new("insert asset version", e))?;

        transaction
            .commit()
            .map_err(|e| PersistenceError::new("commit asset version", e))?;

        Ok(version.clone())
    }

    fn list_asset_versions(
        &mut self,
        asset_id: &str,
        limit: i64,
    ) -> Result<Vec<AssetVersionRecord>, ReviewRepositoryError> {
        let mut stmt = self
            .connection
            .prepare(
                "SELECT * FROM asset_versions WHERE asset_id = ?1 ORDER BY version DESC LIMIT ?2",
            )
            .map_err(|e| PersistenceError::new("prepare asset versions", e))?;
        let rows = stmt
            .query_map(params![asset_id, limit], map_asset_version)
            .map_err(|e| PersistenceError::new("query asset versions", e))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| PersistenceError::new("read asset version", e))?);
        }
        Ok(records)
    }

    fn update_asset_version_status(
        &mut self,
        version_id: &str,
        status: AssetVersionStatus,
        reason: Option<&str>,
    ) -> Result<AssetVersionRecord, ReviewRepositoryError> {
        let now = now_rfc3339()?;
        let affected = self.connection.execute(
            "UPDATE asset_versions SET status = ?1, status_reason = ?2, updated_at = ?3 WHERE id = ?4",
            params![status.as_str(), reason, now, version_id],
        ).map_err(|e| PersistenceError::new("update asset version status", e))?;

        if affected == 0 {
            return Err(ReviewRepositoryError::AssetNotFound(version_id.into()));
        }

        self.connection
            .query_row(
                "SELECT * FROM asset_versions WHERE id = ?1",
                params![version_id],
                map_asset_version,
            )
            .map_err(|e| PersistenceError::new("read updated asset version", e).into())
    }

    // ─────────────────────────────────────────────────
    // 版权
    // ─────────────────────────────────────────────────

    fn upsert_asset_license(
        &mut self,
        license: &AssetLicenseRecord,
    ) -> Result<AssetLicenseRecord, ReviewRepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::new("begin upsert license", e))?;

        transaction
            .execute(
                r#"
                INSERT INTO asset_licenses (
                  id, asset_id, asset_version_id, source_type,
                  provider_id, model_name,
                  commercial_use_status, commercial_use_details,
                  source_assets_json, copyright_statement, attribution_required,
                  risk_flags_json, review_required, review_notes,
                  export_allowed, export_block_reason, created_at, updated_at
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)
                ON CONFLICT (asset_id) DO UPDATE SET
                  asset_version_id = excluded.asset_version_id,
                  source_type = excluded.source_type,
                  provider_id = excluded.provider_id,
                  model_name = excluded.model_name,
                  commercial_use_status = excluded.commercial_use_status,
                  commercial_use_details = excluded.commercial_use_details,
                  source_assets_json = excluded.source_assets_json,
                  copyright_statement = excluded.copyright_statement,
                  attribution_required = excluded.attribution_required,
                  risk_flags_json = excluded.risk_flags_json,
                  review_required = excluded.review_required,
                  review_notes = excluded.review_notes,
                  export_allowed = excluded.export_allowed,
                  export_block_reason = excluded.export_block_reason,
                  updated_at = excluded.updated_at
                "#,
                params![
                    license.id,
                    license.asset_id,
                    license.asset_version_id,
                    license.source_type.as_str(),
                    license.provider_id,
                    license.model_name,
                    license.commercial_use_status.as_str(),
                    license.commercial_use_details,
                    license.source_assets_json,
                    license.copyright_statement,
                    license.attribution_required as i64,
                    license.risk_flags_json,
                    license.review_required as i64,
                    license.review_notes,
                    license.export_allowed as i64,
                    license.export_block_reason,
                    license.created_at,
                    license.updated_at,
                ],
            )
            .map_err(|e| PersistenceError::new("upsert asset license", e))?;

        transaction
            .commit()
            .map_err(|e| PersistenceError::new("commit license", e))?;

        Ok(license.clone())
    }

    fn get_asset_license(
        &mut self,
        asset_id: &str,
    ) -> Result<Option<AssetLicenseRecord>, ReviewRepositoryError> {
        self.connection
            .query_row(
                "SELECT * FROM asset_licenses WHERE asset_id = ?1",
                params![asset_id],
                map_asset_license,
            )
            .optional()
            .map_err(|e| PersistenceError::new("read asset license", e).into())
    }

    fn list_pending_licenses(
        &mut self,
        project_id: &str,
    ) -> Result<Vec<AssetLicenseRecord>, ReviewRepositoryError> {
        let mut stmt = self
            .connection
            .prepare(
                r#"
                SELECT al.* FROM asset_licenses al
                JOIN platform_assets pa ON al.asset_id = pa.id
                WHERE pa.project_id = ?1
                  AND al.commercial_use_status IN ('needs_review', 'restricted', 'blocked')
                ORDER BY al.created_at DESC
                "#,
            )
            .map_err(|e| PersistenceError::new("prepare pending licenses", e))?;
        let rows = stmt
            .query_map(params![project_id], map_asset_license)
            .map_err(|e| PersistenceError::new("query pending licenses", e))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| PersistenceError::new("read license row", e))?);
        }
        Ok(records)
    }

    // ─────────────────────────────────────────────────
    // 内容安全
    // ─────────────────────────────────────────────────

    fn insert_content_guard_report(
        &mut self,
        report: &ContentGuardReportRecord,
    ) -> Result<ContentGuardReportRecord, ReviewRepositoryError> {
        self.connection
            .execute(
                r#"
                INSERT INTO content_guard_reports (
                  id, project_id, target_type, target_id, task_id, asset_id,
                  guard_version, guard_provider,
                  status, risk_level, checks_json, actions_json, created_at
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
                "#,
                params![
                    report.id,
                    report.project_id,
                    report.target_type,
                    report.target_id,
                    report.task_id,
                    report.asset_id,
                    report.guard_version,
                    report.guard_provider,
                    report.status.as_str(),
                    report.risk_level.as_str(),
                    report.checks_json,
                    report.actions_json,
                    report.created_at,
                ],
            )
            .map_err(|e| PersistenceError::new("insert guard report", e))?;

        Ok(report.clone())
    }

    fn get_latest_guard_report(
        &mut self,
        target_type: &str,
        target_id: &str,
    ) -> Result<Option<ContentGuardReportRecord>, ReviewRepositoryError> {
        self.connection
            .query_row(
                "SELECT * FROM content_guard_reports WHERE target_type = ?1 AND target_id = ?2 ORDER BY created_at DESC LIMIT 1",
                params![target_type, target_id],
                map_guard_report,
            )
            .optional()
            .map_err(|e| PersistenceError::new("read guard report", e).into())
    }

    fn list_guard_reports_by_project(
        &mut self,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<ContentGuardReportRecord>, ReviewRepositoryError> {
        let mut stmt = self
            .connection
            .prepare(
                "SELECT * FROM content_guard_reports WHERE project_id = ?1 ORDER BY created_at DESC LIMIT ?2",
            )
            .map_err(|e| PersistenceError::new("prepare guard list", e))?;
        let rows = stmt
            .query_map(params![project_id, limit], map_guard_report)
            .map_err(|e| PersistenceError::new("query guard list", e))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| PersistenceError::new("read guard report", e))?);
        }
        Ok(records)
    }
}

// ──────────────────────────────────────────────────────────────
// 行映射函数
// ──────────────────────────────────────────────────────────────

fn map_review_report(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReviewReportRecord> {
    let reviewer_str: String = row.get(6)?;
    let decision_str: String = row.get(17)?;

    Ok(ReviewReportRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        run_id: row.get(2)?,
        shot_id: row.get(3)?,
        asset_id: row.get(4)?,
        generation_attempt_id: row.get(5)?,
        reviewer_type: ReviewerType::parse(&reviewer_str).unwrap_or(ReviewerType::Auto),
        reviewer_agent_version: row.get(7)?,
        reviewer_provider: row.get(8)?,
        requirement_scores_json: row.get(9)?,
        visual_scores_json: row.get(10)?,
        content_scores_json: row.get(11)?,
        commercial_scores_json: row.get(12)?,
        technical_scores_json: row.get(13)?,
        overall_score: row.get(14)?,
        weighted_score: row.get(15)?,
        issues_json: row.get(16)?,
        decision: ReviewDecision::parse(&decision_str).unwrap_or(ReviewDecision::NeedsReview),
        confidence: row.get(18)?,
        source_task_id: row.get(19)?,
        review_version: row.get(20)?,
        created_at: row.get(21)?,
    })
}

fn map_dimension_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReviewDimensionRecord> {
    let layer_str: String = row.get(2)?;
    Ok(ReviewDimensionRecord {
        id: row.get(0)?,
        review_id: row.get(1)?,
        dimension_layer: ReviewDimensionLayer::parse(&layer_str)
            .unwrap_or(ReviewDimensionLayer::Visual),
        dimension_name: row.get(3)?,
        score: row.get(4)?,
        weight: row.get(5)?,
        confidence: row.get(6)?,
        reasoning: row.get(7)?,
        reference_context_json: row
            .get::<_, Option<String>>(8)?
            .unwrap_or_else(|| "{}".into()),
        created_at: row.get(9)?,
    })
}

fn map_asset_version(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssetVersionRecord> {
    let status_str: String = row.get(13)?;
    Ok(AssetVersionRecord {
        id: row.get(0)?,
        asset_id: row.get(1)?,
        version: row.get(2)?,
        storage_key: row.get(3)?,
        mime_type: row.get(4)?,
        size_bytes: row.get(5)?,
        hash: row.get(6)?,
        width: row.get(7)?,
        height: row.get(8)?,
        duration_seconds: row.get(9)?,
        source_type: row.get(10)?,
        source_task_id: row.get(11)?,
        source_attempt_id: row.get(12)?,
        status: AssetVersionStatus::parse(&status_str).unwrap_or(AssetVersionStatus::Draft),
        status_reason: row.get(14)?,
        review_id: row.get(15)?,
        created_by: row.get(16)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
    })
}

fn map_asset_license(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssetLicenseRecord> {
    let source_str: String = row.get(3)?;
    let status_str: String = row.get(6)?;
    Ok(AssetLicenseRecord {
        id: row.get(0)?,
        asset_id: row.get(1)?,
        asset_version_id: row.get(2)?,
        source_type: LicenseSourceType::parse(&source_str)
            .unwrap_or(LicenseSourceType::AiGenerated),
        provider_id: row.get(4)?,
        model_name: row.get(5)?,
        commercial_use_status: CommercialUseStatus::parse(&status_str)
            .unwrap_or(CommercialUseStatus::Unknown),
        commercial_use_details: row.get(7)?,
        source_assets_json: row.get(8)?,
        copyright_statement: row.get(9)?,
        attribution_required: row.get::<_, i64>(10)? != 0,
        risk_flags_json: row.get(11)?,
        review_required: row.get::<_, i64>(12)? != 0,
        review_notes: row.get(13)?,
        export_allowed: row.get::<_, i64>(14)? != 0,
        export_block_reason: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}

fn map_guard_report(row: &rusqlite::Row<'_>) -> rusqlite::Result<ContentGuardReportRecord> {
    let status_str: String = row.get(8)?;
    let risk_str: String = row.get(9)?;
    Ok(ContentGuardReportRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        target_type: row.get(2)?,
        target_id: row.get(3)?,
        task_id: row.get(4)?,
        asset_id: row.get(5)?,
        guard_version: row.get(6)?,
        guard_provider: row.get(7)?,
        status: ContentGuardStatus::parse(&status_str).unwrap_or(ContentGuardStatus::Passed),
        risk_level: ContentRiskLevel::parse(&risk_str).unwrap_or(ContentRiskLevel::Low),
        checks_json: row.get(10)?,
        actions_json: row.get(11)?,
        created_at: row.get(12)?,
    })
}

// ──────────────────────────────────────────────────────────────
// 测试
// ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::{
        adapters::sqlite::workspace_repository::SqliteWorkspaceRepository,
        domain::workspace::NewWorkspace, ports::workspace_repository::WorkspaceRepository,
    };

    fn seed() -> (tempfile::TempDir, SqliteReviewRepository, String) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("workspace.sqlite3");
        let mut ws = SqliteWorkspaceRepository::open(&path).unwrap();
        ws.initialize(&NewWorkspace::try_new("测试", "老师").unwrap())
            .unwrap();
        drop(ws);
        let repo = SqliteReviewRepository::open(&path).unwrap();
        let pid = "123e4567-e89b-42d3-a456-426614174000".to_owned();
        (dir, repo, pid)
    }

    #[test]
    fn inserts_and_reads_review_report() {
        let (_dir, mut repo, pid) = seed();
        let now = now_rfc3339().unwrap();
        let report = ReviewReportRecord {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: pid,
            run_id: None,
            shot_id: None,
            asset_id: None,
            generation_attempt_id: None,
            reviewer_type: ReviewerType::Auto,
            reviewer_agent_version: Some("ai-critic-v1".into()),
            reviewer_provider: Some("claude-vision".into()),
            requirement_scores_json: r#"{"match":88}"#.into(),
            visual_scores_json: r#"{"composition":85,"color":80}"#.into(),
            content_scores_json: r#"{"themeMatch":90}"#.into(),
            commercial_scores_json: r#"{"platformFit":82}"#.into(),
            technical_scores_json: r#"{"characterConsistency":76}"#.into(),
            overall_score: 82.0,
            weighted_score: None,
            issues_json: r#"[{"dimension":"character_consistency","severity":"medium","message":"face differs","suggestedFix":"add reference"}]"#.into(),
            decision: ReviewDecision::AcceptWithSuggestions,
            confidence: Some(0.85),
            source_task_id: None,
            review_version: 1,
            created_at: now,
        };

        let inserted = repo.insert_report(&report).unwrap();
        assert_eq!(inserted.id, report.id);

        let fetched = repo.get_report(&report.id).unwrap().unwrap();
        assert_eq!(fetched.overall_score, 82.0);
        assert_eq!(fetched.decision, ReviewDecision::AcceptWithSuggestions);
    }

    #[test]
    fn lists_reports_by_project() {
        let (_dir, mut repo, pid) = seed();
        let now = now_rfc3339().unwrap();

        for i in 0..3 {
            let report = ReviewReportRecord {
                id: uuid::Uuid::new_v4().to_string(),
                project_id: pid.clone(),
                run_id: None,
                shot_id: None,
                asset_id: None,
                generation_attempt_id: None,
                reviewer_type: ReviewerType::Auto,
                reviewer_agent_version: None,
                reviewer_provider: None,
                requirement_scores_json: "{}".into(),
                visual_scores_json: "{}".into(),
                content_scores_json: "{}".into(),
                commercial_scores_json: "{}".into(),
                technical_scores_json: "{}".into(),
                overall_score: (80 + i) as f64,
                weighted_score: None,
                issues_json: "[]".into(),
                decision: if i == 2 {
                    ReviewDecision::Regenerate
                } else {
                    ReviewDecision::Accept
                },
                confidence: None,
                source_task_id: None,
                review_version: 1,
                created_at: now.clone(),
            };
            repo.insert_report(&report).unwrap();
        }

        let all = repo.list_reports_by_project(&pid, None, 10).unwrap();
        assert_eq!(all.len(), 3);

        let regenerate = repo
            .list_reports_by_project(&pid, Some(ReviewDecision::Regenerate), 10)
            .unwrap();
        assert_eq!(regenerate.len(), 1);
    }

    #[test]
    fn inserts_and_reads_dimensions() {
        let (_dir, mut repo, pid) = seed();
        let now = now_rfc3339().unwrap();
        let report_id = uuid::Uuid::new_v4().to_string();

        let report = ReviewReportRecord {
            id: report_id.clone(),
            project_id: pid,
            run_id: None,
            shot_id: None,
            asset_id: None,
            generation_attempt_id: None,
            reviewer_type: ReviewerType::Auto,
            reviewer_agent_version: None,
            reviewer_provider: None,
            requirement_scores_json: "{}".into(),
            visual_scores_json: "{}".into(),
            content_scores_json: "{}".into(),
            commercial_scores_json: "{}".into(),
            technical_scores_json: "{}".into(),
            overall_score: 80.0,
            weighted_score: None,
            issues_json: "[]".into(),
            decision: ReviewDecision::Accept,
            confidence: None,
            source_task_id: None,
            review_version: 1,
            created_at: now.clone(),
        };
        repo.insert_report(&report).unwrap();

        let dims = vec![ReviewDimensionRecord {
            id: uuid::Uuid::new_v4().to_string(),
            review_id: report_id.clone(),
            dimension_layer: ReviewDimensionLayer::Visual,
            dimension_name: "composition".into(),
            score: 85.0,
            weight: 1.0,
            confidence: Some(0.9),
            reasoning: Some("good".into()),
            reference_context_json: "{}".into(),
            created_at: now.clone(),
        }];

        repo.insert_dimensions(&dims).unwrap();

        let fetched = repo.list_dimensions_by_report(&report_id).unwrap();
        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].score, 85.0);
    }
}
