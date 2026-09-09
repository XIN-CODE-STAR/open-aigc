//! SemanticRepository 端口 — 语义索引 + 关系候选持久化。
//!
//! 为 Resource Intelligence 层提供持久化接口。
//! 包含 ArtifactSemanticProfile（可重建的语义索引）和
//! RelationCandidate（RAG → Agent → Canvas 的中间层）。
//!
//! 设计原则：
//! - Profile 是可重建的（宪法第 5 条），重新分析会覆盖旧数据
//! - Candidate 是不可变的（insert + status update），保留完整的审查历史
//! - 查询按 tag/embedding/relation_type 索引，满足 RAG 检索需求

use crate::application::error::AppError;
use crate::domain::relation_candidate::{
    CandidateStatus, RelationCandidate, RelationCandidateDraft,
};
use crate::domain::semantic::ArtifactSemanticProfile;
use crate::ports::reloadable::Reloadable;

// ──────────────────────────────────────────────────────────────────
// SemanticRepository trait
// ──────────────────────────────────────────────────────────────────

/// 语义索引 + 关系候选持久化端口。
pub trait SemanticRepository: Send + Sync + Reloadable {
    // ── ArtifactSemanticProfile ──

    /// 保存或更新语义分析结果（upsert）。
    fn upsert_profile(&self, profile: &ArtifactSemanticProfile) -> Result<(), AppError>;

    /// 按 artifact_id 获取语义分析结果。
    fn get_profile(&self, artifact_id: &str) -> Result<Option<ArtifactSemanticProfile>, AppError>;

    /// 删除语义分析结果。
    fn delete_profile(&self, artifact_id: &str) -> Result<(), AppError>;

    /// 按标签名称搜索（模糊匹配，用于 RAG 文本检索）。
    fn find_profiles_by_tag(
        &self,
        tag_query: &str,
        limit: usize,
    ) -> Result<Vec<ArtifactSemanticProfile>, AppError>;

    /// 按 OCR 文本搜索（全文匹配，用于 RAG 文本检索）。
    fn find_profiles_by_ocr(
        &self,
        text_query: &str,
        limit: usize,
    ) -> Result<Vec<ArtifactSemanticProfile>, AppError>;

    /// 按 embedding_id 查找（向量检索入口）。
    fn find_profile_by_embedding(
        &self,
        embedding_id: &str,
    ) -> Result<Option<ArtifactSemanticProfile>, AppError>;

    // ── RelationCandidate ──

    /// 创建关系候选。
    fn create_candidate(
        &self,
        draft: &RelationCandidateDraft,
    ) -> Result<RelationCandidate, AppError>;

    /// 按 ID 获取关系候选。
    fn get_candidate(&self, id: &str) -> Result<Option<RelationCandidate>, AppError>;

    /// 获取待处理的关系候选（pending 状态）。
    fn list_pending_candidates(&self, limit: usize) -> Result<Vec<RelationCandidate>, AppError>;

    /// 获取指定节点相关的关系候选。
    fn list_candidates_for_node(&self, node_id: &str) -> Result<Vec<RelationCandidate>, AppError>;

    /// 更新关系候选状态（accept/reject）。
    fn update_candidate_status(&self, id: &str, status: CandidateStatus) -> Result<(), AppError>;
}
