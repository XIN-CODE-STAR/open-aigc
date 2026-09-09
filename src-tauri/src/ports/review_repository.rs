//! AI Critic Agent 评价持久化端口。
//!
//! 定义评价报告和评价维度的读写操作接口。

use thiserror::Error;

use crate::{
    domain::review::{
        AssetLicenseRecord, AssetVersionRecord, ContentGuardReportRecord, ReviewDecision,
        ReviewDimensionRecord, ReviewReportRecord,
    },
    ports::persistence::PersistenceError,
};

#[derive(Debug, Error)]
pub enum ReviewRepositoryError {
    /// 指定的评价报告不存在。
    #[error("review report {0} does not exist")]
    NotFound(String),
    /// 指定资产不存在。
    #[error("asset {0} does not exist")]
    AssetNotFound(String),
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

/// AI Critic Agent 评价系统持久化端口。
pub trait ReviewRepository: Send {
    // ═══════════════════════════════════════════════════════
    // 评价报告
    // ═══════════════════════════════════════════════════════

    /// 插入一份新的评价报告（含五层 JSON 评分 + issues + decision）。
    fn insert_report(
        &mut self,
        report: &ReviewReportRecord,
    ) -> Result<ReviewReportRecord, ReviewRepositoryError>;

    /// 按 ID 读取单条评价报告。
    fn get_report(
        &mut self,
        report_id: &str,
    ) -> Result<Option<ReviewReportRecord>, ReviewRepositoryError>;

    /// 列出项目的所有评价报告，按 created_at 降序。
    fn list_reports_by_project(
        &mut self,
        project_id: &str,
        decision: Option<ReviewDecision>,
        limit: i64,
    ) -> Result<Vec<ReviewReportRecord>, ReviewRepositoryError>;

    /// 列出某个镜头下的所有评价报告。
    fn list_reports_by_shot(
        &mut self,
        shot_id: &str,
    ) -> Result<Vec<ReviewReportRecord>, ReviewRepositoryError>;

    /// 列出某个资产的所有评价报告。
    fn list_reports_by_asset(
        &mut self,
        asset_id: &str,
    ) -> Result<Vec<ReviewReportRecord>, ReviewRepositoryError>;

    /// 列出某个生成尝试的评价报告。
    fn list_reports_by_attempt(
        &mut self,
        attempt_id: &str,
    ) -> Result<Vec<ReviewReportRecord>, ReviewRepositoryError>;

    // ═══════════════════════════════════════════════════════
    // 评价维度
    // ═══════════════════════════════════════════════════════

    /// 批量插入评价维度记录。
    fn insert_dimensions(
        &mut self,
        dimensions: &[ReviewDimensionRecord],
    ) -> Result<(), ReviewRepositoryError>;

    /// 列出某条评价报告的所有维度详情。
    fn list_dimensions_by_report(
        &mut self,
        review_id: &str,
    ) -> Result<Vec<ReviewDimensionRecord>, ReviewRepositoryError>;

    // ═══════════════════════════════════════════════════════
    // 资产版本
    // ═══════════════════════════════════════════════════════

    /// 插入新的资产版本记录。
    fn insert_asset_version(
        &mut self,
        version: &AssetVersionRecord,
    ) -> Result<AssetVersionRecord, ReviewRepositoryError>;

    /// 列出资产的所有版本，按 version 降序。
    fn list_asset_versions(
        &mut self,
        asset_id: &str,
        limit: i64,
    ) -> Result<Vec<AssetVersionRecord>, ReviewRepositoryError>;

    /// 将资产版本状态更新为终态。
    fn update_asset_version_status(
        &mut self,
        version_id: &str,
        status: crate::domain::review::AssetVersionStatus,
        reason: Option<&str>,
    ) -> Result<AssetVersionRecord, ReviewRepositoryError>;

    // ═══════════════════════════════════════════════════════
    // 版权
    // ═══════════════════════════════════════════════════════

    /// 插入或更新资产版权记录（单资产单条，按 asset_id 唯一）。
    fn upsert_asset_license(
        &mut self,
        license: &AssetLicenseRecord,
    ) -> Result<AssetLicenseRecord, ReviewRepositoryError>;

    /// 获取资产的版权信息。
    fn get_asset_license(
        &mut self,
        asset_id: &str,
    ) -> Result<Option<AssetLicenseRecord>, ReviewRepositoryError>;

    /// 获取项目下所有 need_review 或 restricted 的版权记录。
    fn list_pending_licenses(
        &mut self,
        project_id: &str,
    ) -> Result<Vec<AssetLicenseRecord>, ReviewRepositoryError>;

    // ═══════════════════════════════════════════════════════
    // 内容安全
    // ═══════════════════════════════════════════════════════

    /// 插入内容安全检查报告。
    fn insert_content_guard_report(
        &mut self,
        report: &ContentGuardReportRecord,
    ) -> Result<ContentGuardReportRecord, ReviewRepositoryError>;

    /// 获取某个目标的最新安全检查报告。
    fn get_latest_guard_report(
        &mut self,
        target_type: &str,
        target_id: &str,
    ) -> Result<Option<ContentGuardReportRecord>, ReviewRepositoryError>;

    /// 列出项目的所有安全检查报告。
    fn list_guard_reports_by_project(
        &mut self,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<ContentGuardReportRecord>, ReviewRepositoryError>;
}
