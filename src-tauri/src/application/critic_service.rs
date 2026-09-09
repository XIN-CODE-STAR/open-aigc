//! AI Critic Agent v1 — 多维审美评价服务（增强版）。
//!
//! 职责：
//! - 对生成资产进行五层评分（需求层、视觉层、内容层、商业层、技术层）。
//! - 输出结构化 ReviewReportRecord（含 issues 和 decision）。
//! - 支持视频生成前的质量门槛检查。
//! - 资产版本、版权查询、内容安全检查。
//! - 包含 LLM Vision API 调用用的 Prompt 模板。
//! - 构建预定义的 Content Guard 规则库。

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Mutex;

use crate::{
    adapters::sqlite::review_repository::SqliteReviewRepository,
    application::error::AppError,
    domain::review::{
        CommercialScores, ContentGuardReportRecord, ContentGuardStatus, ContentRiskLevel,
        ContentScores, RequirementScores, ReviewDecision, ReviewDimensionLayer,
        ReviewDimensionRecord, ReviewIssue, ReviewReportRecord, ReviewerType, TechnicalScores,
        VideoGenerationGate, VisualScores,
    },
    ports::review_repository::ReviewRepository,
};

pub struct CriticService {
    repository: Mutex<Box<dyn ReviewRepository>>,
    #[allow(dead_code)]
    database_path: PathBuf,
}

impl CriticService {
    pub fn new(repository: impl ReviewRepository + 'static, database_path: PathBuf) -> Self {
        Self {
            repository: Mutex::new(Box::new(repository)),
            database_path,
        }
    }

    // ═══════════════════════════════════════════════════════
    // 评价报告
    // ═══════════════════════════════════════════════════════

    /// 生成并保存一份 AI Critic 评价报告。
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate_asset(
        &self,
        project_id: String,
        asset_id: String,
        generation_attempt_id: Option<String>,
        requirement_scores: RequirementScores,
        visual_scores: VisualScores,
        content_scores: ContentScores,
        commercial_scores: CommercialScores,
        technical_scores: TechnicalScores,
        issues: Vec<ReviewIssue>,
        reviewer_provider: Option<String>,
    ) -> Result<ReviewReportRecord, AppError> {
        let now = crate::adapters::sqlite::now_rfc3339()?;
        let report_id = uuid::Uuid::new_v4().to_string();

        let req_json = serde_json::to_string(&requirement_scores)
            .map_err(|e| AppError::new("serialize requirement scores", e))?;
        let vis_json = serde_json::to_string(&visual_scores)
            .map_err(|e| AppError::new("serialize visual scores", e))?;
        let con_json = serde_json::to_string(&content_scores)
            .map_err(|e| AppError::new("serialize content scores", e))?;
        let com_json = serde_json::to_string(&commercial_scores)
            .map_err(|e| AppError::new("serialize commercial scores", e))?;
        let tec_json = serde_json::to_string(&technical_scores)
            .map_err(|e| AppError::new("serialize technical scores", e))?;
        let issues_json =
            serde_json::to_string(&issues).map_err(|e| AppError::new("serialize issues", e))?;

        let overall = compute_overall_score(
            &requirement_scores,
            &visual_scores,
            &content_scores,
            &commercial_scores,
            &technical_scores,
        );
        let decision = ReviewDecision::from_overall_score(overall);

        let report = ReviewReportRecord {
            id: report_id.clone(),
            project_id,
            run_id: None,
            shot_id: None,
            asset_id: Some(asset_id),
            generation_attempt_id,
            reviewer_type: ReviewerType::Auto,
            reviewer_agent_version: Some("ai-critic-v1.0".into()),
            reviewer_provider,
            requirement_scores_json: req_json,
            visual_scores_json: vis_json,
            content_scores_json: con_json,
            commercial_scores_json: com_json,
            technical_scores_json: tec_json,
            overall_score: overall,
            weighted_score: None,
            issues_json,
            decision,
            confidence: Some(0.80),
            source_task_id: None,
            review_version: 1,
            created_at: now,
        };

        self.with_repository(|repo| repo.insert_report(&report).map_err(Into::into))
    }

    /// 生成并保存维度详情记录。
    pub fn save_dimensions(
        &self,
        report_id: &str,
        _reviewer_provider: Option<&str>,
    ) -> Result<Vec<ReviewDimensionRecord>, AppError> {
        let report = self
            .get_report(report_id)?
            .ok_or_else(|| AppError::new("report not found", std::io::Error::other("")))?;

        let now = crate::adapters::sqlite::now_rfc3339()?;
        let mut dimensions = Vec::new();

        // 从五层 JSON 中解析各维度
        if let Ok(req) = serde_json::from_str::<RequirementScores>(&report.requirement_scores_json)
        {
            push_dim(
                &mut dimensions,
                report_id,
                ReviewDimensionLayer::Requirement,
                "match",
                req.requirement_match,
                1.5,
                &now,
            );
            push_dim(
                &mut dimensions,
                report_id,
                ReviewDimensionLayer::Requirement,
                "completeness",
                req.completeness,
                0.5,
                &now,
            );
            push_dim(
                &mut dimensions,
                report_id,
                ReviewDimensionLayer::Requirement,
                "clarity",
                req.clarity,
                0.5,
                &now,
            );
        }
        if let Ok(vis) = serde_json::from_str::<VisualScores>(&report.visual_scores_json) {
            push_dim(
                &mut dimensions,
                report_id,
                ReviewDimensionLayer::Visual,
                "composition",
                vis.composition,
                0.6,
                &now,
            );
            push_dim(
                &mut dimensions,
                report_id,
                ReviewDimensionLayer::Visual,
                "color",
                vis.color,
                0.4,
                &now,
            );
            push_dim(
                &mut dimensions,
                report_id,
                ReviewDimensionLayer::Visual,
                "lighting",
                vis.lighting,
                0.4,
                &now,
            );
            push_dim(
                &mut dimensions,
                report_id,
                ReviewDimensionLayer::Visual,
                "texture",
                vis.texture,
                0.3,
                &now,
            );
            push_dim(
                &mut dimensions,
                report_id,
                ReviewDimensionLayer::Visual,
                "lens_language",
                vis.lens_language,
                0.3,
                &now,
            );
        }
        if let Ok(con) = serde_json::from_str::<ContentScores>(&report.content_scores_json) {
            push_dim(
                &mut dimensions,
                report_id,
                ReviewDimensionLayer::Content,
                "theme_match",
                con.theme_match,
                0.8,
                &now,
            );
            push_dim(
                &mut dimensions,
                report_id,
                ReviewDimensionLayer::Content,
                "emotion_expression",
                con.emotion_expression,
                0.4,
                &now,
            );
            push_dim(
                &mut dimensions,
                report_id,
                ReviewDimensionLayer::Content,
                "narrative_purpose",
                con.narrative_purpose,
                0.3,
                &now,
            );
        }
        if let Ok(com) = serde_json::from_str::<CommercialScores>(&report.commercial_scores_json) {
            push_dim(
                &mut dimensions,
                report_id,
                ReviewDimensionLayer::Commercial,
                "platform_fit",
                com.platform_fit,
                0.2,
                &now,
            );
            push_dim(
                &mut dimensions,
                report_id,
                ReviewDimensionLayer::Commercial,
                "audience_fit",
                com.audience_fit,
                0.2,
                &now,
            );
            push_dim(
                &mut dimensions,
                report_id,
                ReviewDimensionLayer::Commercial,
                "conversion_potential",
                com.conversion_potential,
                0.1,
                &now,
            );
        }
        if let Ok(tec) = serde_json::from_str::<TechnicalScores>(&report.technical_scores_json) {
            push_dim(
                &mut dimensions,
                report_id,
                ReviewDimensionLayer::Technical,
                "clarity",
                tec.clarity,
                0.5,
                &now,
            );
            push_dim(
                &mut dimensions,
                report_id,
                ReviewDimensionLayer::Technical,
                "distortion",
                tec.distortion,
                0.3,
                &now,
            );
            push_dim(
                &mut dimensions,
                report_id,
                ReviewDimensionLayer::Technical,
                "character_consistency",
                tec.character_consistency,
                0.5,
                &now,
            );
            push_dim(
                &mut dimensions,
                report_id,
                ReviewDimensionLayer::Technical,
                "motion_quality",
                tec.motion_quality,
                0.2,
                &now,
            );
        }

        if !dimensions.is_empty() {
            self.with_repository(|repo| repo.insert_dimensions(&dimensions).map_err(Into::into))?;
        }
        Ok(dimensions)
    }

    pub fn get_report(&self, report_id: &str) -> Result<Option<ReviewReportRecord>, AppError> {
        self.with_repository(|repo| repo.get_report(report_id).map_err(Into::into))
    }

    pub fn list_reports_by_project(
        &self,
        project_id: &str,
        decision: Option<ReviewDecision>,
        limit: i64,
    ) -> Result<Vec<ReviewReportRecord>, AppError> {
        self.with_repository(|repo| {
            repo.list_reports_by_project(project_id, decision, limit)
                .map_err(Into::into)
        })
    }

    pub fn list_reports_by_shot(&self, shot_id: &str) -> Result<Vec<ReviewReportRecord>, AppError> {
        self.with_repository(|repo| repo.list_reports_by_shot(shot_id).map_err(Into::into))
    }

    pub fn list_reports_by_asset(
        &self,
        asset_id: &str,
    ) -> Result<Vec<ReviewReportRecord>, AppError> {
        self.with_repository(|repo| repo.list_reports_by_asset(asset_id).map_err(Into::into))
    }

    pub fn list_dimensions(&self, review_id: &str) -> Result<Vec<ReviewDimensionRecord>, AppError> {
        self.with_repository(|repo| {
            repo.list_dimensions_by_report(review_id)
                .map_err(Into::into)
        })
    }

    // ═══════════════════════════════════════════════════════
    // 资产版本与版权
    // ═══════════════════════════════════════════════════════

    pub fn list_asset_versions(
        &self,
        asset_id: &str,
        limit: i64,
    ) -> Result<Vec<crate::domain::review::AssetVersionRecord>, AppError> {
        self.with_repository(|repo| {
            repo.list_asset_versions(asset_id, limit)
                .map_err(Into::into)
        })
    }

    pub fn get_asset_license(
        &self,
        asset_id: &str,
    ) -> Result<Option<crate::domain::review::AssetLicenseRecord>, AppError> {
        self.with_repository(|repo| repo.get_asset_license(asset_id).map_err(Into::into))
    }

    // ═══════════════════════════════════════════════════════
    // 内容安全
    // ═══════════════════════════════════════════════════════

    /// 运行 Content Guard 规则引擎（本地规则，不依赖 LLM）。
    /// 返回结构化安全检查报告。
    pub fn run_content_guard_check(
        &self,
        project_id: String,
        target_type: &str,
        target_id: Option<&str>,
        content_text: Option<&str>,
    ) -> Result<ContentGuardReportRecord, AppError> {
        let now = crate::adapters::sqlite::now_rfc3339()?;
        let report_id = uuid::Uuid::new_v4().to_string();

        // 运行本地规则引擎
        let (risk_level, checks, status) = ContentGuardRules::evaluate(target_type, content_text);

        let actions = ContentGuardRules::derived_actions(status);

        let report = ContentGuardReportRecord {
            id: report_id.clone(),
            project_id,
            target_type: target_type.to_owned(),
            target_id: target_id.map(|s| s.to_owned()),
            task_id: None,
            asset_id: None,
            guard_version: Some("guard-v1.0".into()),
            guard_provider: Some("local-rules".into()),
            status,
            risk_level,
            checks_json: serde_json::to_string(&checks).unwrap_or_else(|_| "[]".into()),
            actions_json: serde_json::to_string(&actions).unwrap_or_else(|_| "{}".into()),
            created_at: now,
        };

        self.with_repository(|repo| {
            repo.insert_content_guard_report(&report)
                .map_err(Into::into)
        })
    }

    pub fn get_latest_guard_report(
        &self,
        target_type: &str,
        target_id: &str,
    ) -> Result<Option<ContentGuardReportRecord>, AppError> {
        self.with_repository(|repo| {
            repo.get_latest_guard_report(target_type, target_id)
                .map_err(Into::into)
        })
    }

    // ═══════════════════════════════════════════════════════
    // 视频生成门槛
    // ═══════════════════════════════════════════════════════

    pub fn check_video_generation_gate(report: &ReviewReportRecord) -> (bool, Vec<String>) {
        let gate = VideoGenerationGate::default();
        let passed = gate.passes(report);

        let mut failed = Vec::new();
        if report.overall_score < gate.overall_min {
            failed.push(format!(
                "综合评分 {:.0} < {}",
                report.overall_score, gate.overall_min
            ));
        }
        if let Ok(tech) = serde_json::from_str::<TechnicalScores>(&report.technical_scores_json) {
            if tech.character_consistency.unwrap_or(0.0) < gate.character_consistency_min {
                failed.push(format!(
                    "角色一致性 {:.0} < {}",
                    tech.character_consistency.unwrap_or(0.0),
                    gate.character_consistency_min
                ));
            }
        }
        if let Ok(content) = serde_json::from_str::<ContentScores>(&report.content_scores_json) {
            if content.theme_match.unwrap_or(0.0) < gate.style_consistency_min {
                failed.push(format!(
                    "主题匹配度 {:.0} < {}",
                    content.theme_match.unwrap_or(0.0),
                    gate.style_consistency_min
                ));
            }
        }
        if let Ok(req) = serde_json::from_str::<RequirementScores>(&report.requirement_scores_json)
        {
            if req.requirement_match.unwrap_or(0.0) < gate.requirement_match_min {
                failed.push(format!(
                    "需求匹配度 {:.0} < {}",
                    req.requirement_match.unwrap_or(0.0),
                    gate.requirement_match_min
                ));
            }
        }
        (passed, failed)
    }

    // ─────────────────────────────────────────────────────
    // 内部
    // ─────────────────────────────────────────────────────

    fn with_repository<T>(
        &self,
        operation: impl FnOnce(&mut dyn ReviewRepository) -> Result<T, AppError>,
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

impl crate::ports::reloadable::Reloadable for CriticService {
    fn reload(&self, database_path: &std::path::Path) -> Result<(), AppError> {
        let new_repo = SqliteReviewRepository::open(database_path)?;
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        *repo = Box::new(new_repo);
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────
// Content Guard 规则引擎
// ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct GuardCheck {
    category: String,
    status: String, // "pass" | "warn" | "flag" | "block"
    message: String,
}

#[derive(Debug, Clone)]
struct GuardActions {
    allowed: Vec<String>,
    blocked: Vec<String>,
}

pub struct ContentGuardRules;

impl ContentGuardRules {
    /// 运行规则引擎，返回 (risk_level, checks_json, status)。
    pub fn evaluate(
        target_type: &str,
        content_text: Option<&str>,
    ) -> (ContentRiskLevel, Vec<serde_json::Value>, ContentGuardStatus) {
        let mut checks = Vec::new();
        let text = content_text.unwrap_or("").to_lowercase();
        let mut overall = ContentGuardStatus::Passed;

        // 规则 1: 敏感政治内容关键词
        let political_keywords = [
            "革命", "推翻", "暴力", "恐怖", "独裁", "专政", "分裂", "颠覆", "叛国", "邪教",
        ];
        let hit_political: Vec<_> = political_keywords
            .iter()
            .filter(|kw| text.contains(*kw))
            .map(|kw| kw.to_string())
            .collect();
        if !hit_political.is_empty() {
            overall = ContentGuardStatus::Blocked;
            checks.push(serde_json::json!({
                "category": "political_sensitivity",
                "status": "block",
                "message": format!("检测到敏感政治关键词：{}", hit_political.join(", ")),
                "detail": "内容包含需要人工复核的表述，已自动阻断。"
            }));
        }

        // 规则 2: 色情/暴力内容识别
        let nsfw_keywords = [
            "色情",
            "暴力",
            "血腥",
            "裸露",
            "性",
            "毒品",
            "赌场",
            "色情服务",
        ];
        let hit_nsfw: Vec<_> = nsfw_keywords
            .iter()
            .filter(|kw| text.contains(*kw))
            .map(|kw| kw.to_string())
            .collect();
        if !hit_nsfw.is_empty() {
            if overall != ContentGuardStatus::Blocked {
                overall = ContentGuardStatus::Flagged;
            }
            checks.push(serde_json::json!({
                "category": "nsfw_content",
                "status": "flag",
                "message": format!("检测到敏感内容关键词：{}", hit_nsfw.join(", ")),
                "detail": "内容可能不符合平台安全策略，已标记。"
            }));
        }

        // 规则 3: 品牌一致性（企业宣传片场景）
        if target_type == "deliverable" {
            let brand_inconsistent = ["山寨", "便宜", "劣质", "假冒", "侵权"];
            let hit_brand: Vec<_> = brand_inconsistent
                .iter()
                .filter(|kw| text.contains(*kw))
                .map(|kw| kw.to_string())
                .collect();
            if !hit_brand.is_empty() {
                checks.push(serde_json::json!({
                    "category": "brand_consistency",
                    "status": "warn",
                    "message": format!("检测到品牌负面词汇：{}，请确认是否合适。", hit_brand.join(", ")),
                    "detail": "这些词汇可能与企业品牌定位不一致。"
                }));
            }
        }

        // 规则 4: 教育场景特殊检查
        if target_type == "script" || target_type == "generated_prompt" {
            let edu_risks = ["作弊", "逃课", "打架", "霸凌", "自杀"];
            let hit_edu: Vec<_> = edu_risks
                .iter()
                .filter(|kw| text.contains(*kw))
                .map(|kw| kw.to_string())
                .collect();
            if !hit_edu.is_empty() {
                checks.push(serde_json::json!({
                    "category": "educational_safety",
                    "status": "warn",
                    "message": format!("教育场景检测到风险词汇：{}，已警告。", hit_edu.join(", ")),
                    "detail": "建议替换为积极向上的表达。"
                }));
            }
        }

        // 确定最终风险等级
        let risk_level = match overall {
            ContentGuardStatus::Blocked => ContentRiskLevel::Critical,
            ContentGuardStatus::Flagged => ContentRiskLevel::High,
            ContentGuardStatus::NeedsReview => ContentRiskLevel::Medium,
            ContentGuardStatus::Passed => {
                if checks.iter().any(|c| c["status"].as_str() == Some("warn")) {
                    ContentRiskLevel::Medium
                } else {
                    ContentRiskLevel::Low
                }
            }
        };

        (risk_level, checks, overall)
    }

    /// 根据安全状态推导允许/阻止的操作。
    pub fn derived_actions(status: ContentGuardStatus) -> serde_json::Value {
        match status {
            ContentGuardStatus::Blocked => serde_json::json!({
                "allowed": ["revise", "manual_review"],
                "blocked": ["export_final", "publish", "share"]
            }),
            ContentGuardStatus::Flagged => serde_json::json!({
                "allowed": ["revise", "manual_review"],
                "blocked": ["export_final", "publish"]
            }),
            ContentGuardStatus::NeedsReview => serde_json::json!({
                "allowed": ["revise", "manual_review"],
                "blocked": ["export_final"]
            }),
            ContentGuardStatus::Passed => serde_json::json!({
                "allowed": ["export_final", "publish", "share", "revise"],
                "blocked": []
            }),
        }
    }
}

// ──────────────────────────────────────────────────────────────
// AI Critic 评测 Prompt 模板
// ──────────────────────────────────────────────────────────────

/// LLM Vision API 调用用的评测 Prompt 模板。
pub struct CriticPromptTemplates;

impl CriticPromptTemplates {
    /// 图片关键帧多维评价 System Prompt。
    pub fn image_keyframe_system() -> &'static str {
        r#"
你是一个专业的 AI 影视制作审片人（AI Film Critic）。

你的任务是对 AI 生成的图片/关键帧进行五层结构性评价，并给出明确的修改建议。

评价维度：
1. 需求层：是否符合用户原始创作目标、受众定位、核心信息。
2. 视觉层：构图（三分法/黄金分割）、色彩（和谐度/情绪）、光线（方向/层次）、材质质感、镜头语言。
3. 内容层：主题匹配度、情绪表达、叙事目的。
4. 商业层：平台适配性（抖音/B站/企业宣传片）、受众吸引力。
5. 技术层：清晰度、角色一致性、畸变、运动质量（视频时）。

输出格式（严格 JSON）：
{
  "requirement_scores": { "match": 0-100, "completeness": 0-100, "clarity": 0-100 },
  "visual_scores": { "composition": 0-100, "color": 0-100, "lighting": 0-100, "texture": 0-100, "lens_language": 0-100 },
  "content_scores": { "theme_match": 0-100, "emotion_expression": 0-100, "narrative_purpose": 0-100 },
  "commercial_scores": { "platform_fit": 0-100, "audience_fit": 0-100, "conversion_potential": 0-100 },
  "technical_scores": { "clarity": 0-100, "distortion": 0-100, "character_consistency": 0-100, "motion_quality": 0-100 },
  "issues": [
    { "dimension": "string", "severity": "low|medium|high|critical", "message": "具体问题描述", "suggested_fix": "修改建议" }
  ],
  "overall_comment": "一句话总结评价",
  "confidence": 0.0-1.0
}

评分标准：
- 90-100：优秀，可直接采纳。
- 80-89：良好，有轻微优化建议。
- 70-79：基本可用，建议局部修改。
- 60-69：需要大幅改进，建议重新生成。
- < 60：不可用，阻断进入下一阶段。
"#
    }

    /// 角色一致性评价 Prompt 模板。
    pub fn character_consistency(character_name: &str, character_profile: &str) -> String {
        format!(
            r#"
请检查以下生成图像中的角色 "{character_name}" 是否与角色设定一致。

角色设定：
{character_profile}

检查要点：
- 面部特征：脸型、五官比例是否一致。
- 服装配饰：是否穿着正确的服装和配饰。
- 体型比例：身高、体型是否匹配。
- 风格锁定：是否遵守 style_lock 中定义的风格约束。
- 禁止事项：是否出现禁止的要素。

输出 JSON：
{{
  "character_name": "{character_name}",
  "face_match": 0-100,
  "clothing_match": 0-100,
  "body_proportion_match": 0-100,
  "style_lock_match": 0-100,
  "overall_consistency": 0-100,
  "deviations": ["具体不一致之处"],
  "confidence": 0.0-1.0
}}
"#
        )
    }

    /// 风格一致性评价 Prompt 模板。
    pub fn style_consistency(visual_spec_json: &str) -> String {
        format!(
            r#"
请检查以下生成图像是否符合统一的视觉规范。

视觉规范：
{visual_spec_json}

检查要点：
- 色彩体系：是否与定义的 color_palette 一致。
- 构图规则：是否遵守 composition 定义。
- 光线方案：光源方向、强度、情绪是否符合 lighting 规范。
- 材质处理：是否与 texture 定义一致。
- 禁止风格：是否出现 negative_style 中的元素。

输出 JSON：
{{
  "color_match": 0-100,
  "composition_match": 0-100,
  "lighting_match": 0-100,
  "texture_match": 0-100,
  "negative_style_violations": ["违规项"],
  "overall_style_score": 0-100,
  "confidence": 0.0-1.0
}}
"#
        )
    }

    /// 需求匹配度评价 Prompt 模板。
    pub fn requirement_match(user_goal: &str, requirement_spec_json: &str) -> String {
        format!(
            r#"
用户原始需求：
"{user_goal}"

结构化需求规格：
{requirement_spec_json}

请检查生成结果是否符合用户的需求和结构化规格。

检查要点：
- must_have 清单的完成度。
- must_not_have 清单的遵守。
- 受众匹配：是否针对正确的目标受众。
- 情绪传达：tone 是否准确。
- 平台适配：是否适合目标平台。

输出 JSON：
{{
  "must_have_completed": ["已完成项"],
  "must_have_missing": ["缺失项"],
  "must_not_have_violations": ["违规项"],
  "audience_match": 0-100,
  "tone_match": 0-100,
  "platform_fit": 0-100,
  "overall_requirement_match": 0-100,
  "confidence": 0.0-1.0
}}
"#
        )
    }
}

// ─────────────────────────────────────────────────────
// 工具函数
// ─────────────────────────────────────────────────────

fn push_dim(
    dimensions: &mut Vec<ReviewDimensionRecord>,
    review_id: &str,
    layer: ReviewDimensionLayer,
    name: &str,
    score: Option<f64>,
    weight: f64,
    now: &str,
) {
    if let Some(s) = score {
        dimensions.push(ReviewDimensionRecord {
            id: uuid::Uuid::new_v4().to_string(),
            review_id: review_id.to_owned(),
            dimension_layer: layer,
            dimension_name: name.to_owned(),
            score: s,
            weight,
            confidence: Some(0.80),
            reasoning: None,
            reference_context_json: "{}".into(),
            created_at: now.to_owned(),
        });
    }
}

fn compute_overall_score(
    req: &RequirementScores,
    vis: &VisualScores,
    con: &ContentScores,
    com: &CommercialScores,
    tec: &TechnicalScores,
) -> f64 {
    let mut total = 0.0;
    let mut weight = 0.0;

    add_weighted(&mut total, &mut weight, req.requirement_match, 1.5);
    add_weighted(&mut total, &mut weight, req.completeness, 0.5);
    add_weighted(&mut total, &mut weight, req.clarity, 0.5);

    add_weighted(&mut total, &mut weight, vis.composition, 0.6);
    add_weighted(&mut total, &mut weight, vis.color, 0.4);
    add_weighted(&mut total, &mut weight, vis.lighting, 0.4);
    add_weighted(&mut total, &mut weight, vis.texture, 0.3);
    add_weighted(&mut total, &mut weight, vis.lens_language, 0.3);

    add_weighted(&mut total, &mut weight, con.theme_match, 0.8);
    add_weighted(&mut total, &mut weight, con.emotion_expression, 0.4);
    add_weighted(&mut total, &mut weight, con.narrative_purpose, 0.3);

    add_weighted(&mut total, &mut weight, com.platform_fit, 0.2);
    add_weighted(&mut total, &mut weight, com.audience_fit, 0.2);
    add_weighted(&mut total, &mut weight, com.conversion_potential, 0.1);

    add_weighted(&mut total, &mut weight, tec.clarity, 0.5);
    add_weighted(&mut total, &mut weight, tec.distortion, 0.3);
    add_weighted(&mut total, &mut weight, tec.character_consistency, 0.5);
    add_weighted(&mut total, &mut weight, tec.motion_quality, 0.2);

    if weight == 0.0 {
        0.0
    } else {
        (total / weight).clamp(0.0, 100.0)
    }
}

fn add_weighted(total: &mut f64, weight: &mut f64, score: Option<f64>, w: f64) {
    if let Some(s) = score {
        *total += s * w;
        *weight += w;
    }
}
