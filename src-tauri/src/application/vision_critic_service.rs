use std::sync::Mutex;

use crate::{
    application::credential_service::CredentialService,
    application::critic_service::CriticService,
    application::error::AppError,
    domain::review::{
        CommercialScores, ContentScores, IssueSeverity, RequirementScores, ReviewDecision,
        ReviewIssue, ReviewReportRecord, TechnicalScores, VisualScores,
    },
    ports::vision_adapter::{VisionAdapter, VisionImageInput},
};

/// Vision Critic Service：集成 LLM Vision API 的 AI Critic 服务。
///
/// 在 CriticService 基础上增加：
/// - 调用 Claude/GPT Vision API 进行自动评价
/// - 解析 LLM 返回的结构化评分
/// - 保存评价报告和维度详情
/// - 从 CredentialService 自动读取 API key
pub struct VisionCriticService {
    critic_service: CriticService,
    credential_service: CredentialService,
    vision_adapters: Mutex<Vec<Box<dyn VisionAdapter>>>,
    default_adapter_id: String,
    default_model: String,
    default_base_url: Option<String>,
}

impl VisionCriticService {
    pub fn new(
        critic_service: CriticService,
        credential_service: CredentialService,
        vision_adapters: Vec<Box<dyn VisionAdapter>>,
        default_adapter_id: String,
        default_model: String,
        default_base_url: Option<String>,
    ) -> Self {
        Self {
            critic_service,
            credential_service,
            vision_adapters: Mutex::new(vision_adapters),
            default_adapter_id,
            default_model,
            default_base_url,
        }
    }

    /// 从 CredentialService 查找匹配的 API key。
    /// 根据 adapter_id 推断 provider_name，然后查找对应的凭据。
    fn resolve_api_key(&self, adapter_id: &str) -> Result<(String, String, String), AppError> {
        let provider_hint = match adapter_id {
            "claude-vision" => "anthropic",
            "gpt-vision" => "openai",
            _ => adapter_id,
        };

        let credentials = self.credential_service.list()?;
        let credential = credentials
            .iter()
            .find(|c| {
                c.enabled
                    && (c.provider_name.to_lowercase().contains(provider_hint)
                        || c.model_name.to_lowercase().contains("vision")
                        || c.model_name.to_lowercase().contains("claude")
                        || c.model_name.to_lowercase().contains("gpt-4"))
            })
            .ok_or_else(|| {
                AppError::new(
                    "vision credential not found",
                    std::io::Error::other(format!(
                        "未找到匹配 '{provider_hint}' 的已启用凭据。请在管理端配置 API 凭据。"
                    )),
                )
            })?;

        // 通过 CredentialService 获取密钥（不直接访问 repository）
        let api_key = self
            .credential_service
            .get_secret(&credential.credential_key)?;

        Ok((
            api_key,
            credential.base_url.clone(),
            credential.model_name.clone(),
        ))
    }

    /// 使用 LLM Vision API 评价一张图片。
    ///
    /// 如果未提供 api_key，自动从 CredentialService 查找。
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate_with_vision(
        &self,
        project_id: &str,
        asset_id: &str,
        image_url_or_base64: &str,
        user_goal: Option<&str>,
        adapter_id: Option<&str>,
        model: Option<&str>,
        api_key: Option<&str>,
    ) -> Result<ReviewReportRecord, AppError> {
        let adapter_id = adapter_id.unwrap_or(&self.default_adapter_id);
        let requested_model = model.unwrap_or(&self.default_model);

        // 如果未提供 API key，从凭据服务自动查找
        let (resolved_key, credential_base_url, resolved_model) = if let Some(api_key) = api_key {
            (api_key.to_owned(), None, requested_model.to_owned())
        } else {
            let (key, base_url, resolved_model) = self.resolve_api_key(adapter_id)?;
            (key, Some(base_url), resolved_model)
        };
        let base_url_str = self
            .default_base_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                credential_base_url
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
            });
        let model_str = if model.is_some() {
            requested_model
        } else {
            &resolved_model
        };

        // 构建评测 Prompt
        let system_prompt =
            crate::application::critic_service::CriticPromptTemplates::image_keyframe_system();
        let user_prompt = user_goal.unwrap_or("请对这张图片进行五层多维评价。");

        // 调用 Vision API
        let result = {
            let adapters = self
                .vision_adapters
                .lock()
                .map_err(|_| AppError::StateUnavailable)?;
            let adapter = adapters
                .iter()
                .find(|a| a.adapter_id() == adapter_id)
                .ok_or_else(|| {
                    AppError::new(
                        "vision adapter not found",
                        std::io::Error::other(format!("adapter '{adapter_id}' not found")),
                    )
                })?;

            adapter.evaluate_image(
                image_url_or_base64,
                system_prompt,
                user_prompt,
                model_str,
                &resolved_key,
                base_url_str,
            )?
        };

        // 解析 LLM 返回的 JSON 结构
        let parsed: serde_json::Value = serde_json::from_str(&result.raw_json)
            .map_err(|e| AppError::new("parse vision result", e))?;

        let requirement_scores = parse_requirement_scores(&parsed);
        let visual_scores = parse_visual_scores(&parsed);
        let content_scores = parse_content_scores(&parsed);
        let commercial_scores = parse_commercial_scores(&parsed);
        let technical_scores = parse_technical_scores(&parsed);
        let issues = parse_issues(&parsed);

        // 保存评价报告
        self.critic_service.evaluate_asset(
            project_id.to_owned(),
            asset_id.to_owned(),
            None, // generation_attempt_id
            requirement_scores,
            visual_scores,
            content_scores,
            commercial_scores,
            technical_scores,
            issues,
            Some(adapter_id.to_owned()),
        )
    }

    /// 批量评价关键帧（用于分镜到视频的质量门槛检查）。
    pub fn evaluate_keyframes_batch(
        &self,
        project_id: &str,
        images: &[VisionImageInput],
        user_goal: Option<&str>,
    ) -> Result<Vec<ReviewReportRecord>, AppError> {
        let mut reports = Vec::with_capacity(images.len());
        for image in images {
            let report = self.evaluate_with_vision(
                project_id,
                image.asset_id.as_deref().unwrap_or("unknown"),
                &image.url_or_base64,
                user_goal,
                None,
                None,
                None,
            )?;
            reports.push(report);
        }
        Ok(reports)
    }

    /// 委托给内部 CriticService 的方法。
    pub fn get_report(&self, report_id: &str) -> Result<Option<ReviewReportRecord>, AppError> {
        self.critic_service.get_report(report_id)
    }

    pub fn list_reports_by_project(
        &self,
        project_id: &str,
        decision: Option<ReviewDecision>,
        limit: i64,
    ) -> Result<Vec<ReviewReportRecord>, AppError> {
        self.critic_service
            .list_reports_by_project(project_id, decision, limit)
    }

    pub fn check_video_generation_gate(report: &ReviewReportRecord) -> (bool, Vec<String>) {
        CriticService::check_video_generation_gate(report)
    }
}

// ─────────────────────────────────────────────────────
// JSON 解析辅助函数
// ─────────────────────────────────────────────────────

fn parse_requirement_scores(parsed: &serde_json::Value) -> RequirementScores {
    let req = parsed
        .get("requirement_scores")
        .or_else(|| parsed.get("requirementScores"));
    RequirementScores {
        requirement_match: req
            .and_then(|v| v.get("match").or_else(|| v.get("requirement_match")))
            .and_then(|v| v.as_f64()),
        completeness: req
            .and_then(|v| v.get("completeness"))
            .and_then(|v| v.as_f64()),
        clarity: req.and_then(|v| v.get("clarity")).and_then(|v| v.as_f64()),
    }
}

fn parse_visual_scores(parsed: &serde_json::Value) -> VisualScores {
    let vis = parsed
        .get("visual_scores")
        .or_else(|| parsed.get("visualScores"));
    VisualScores {
        composition: vis
            .and_then(|v| v.get("composition"))
            .and_then(|v| v.as_f64()),
        color: vis.and_then(|v| v.get("color")).and_then(|v| v.as_f64()),
        lighting: vis.and_then(|v| v.get("lighting")).and_then(|v| v.as_f64()),
        texture: vis.and_then(|v| v.get("texture")).and_then(|v| v.as_f64()),
        lens_language: vis
            .and_then(|v| v.get("lens_language").or_else(|| v.get("lensLanguage")))
            .and_then(|v| v.as_f64()),
    }
}

fn parse_content_scores(parsed: &serde_json::Value) -> ContentScores {
    let con = parsed
        .get("content_scores")
        .or_else(|| parsed.get("contentScores"));
    ContentScores {
        theme_match: con
            .and_then(|v| v.get("theme_match").or_else(|| v.get("themeMatch")))
            .and_then(|v| v.as_f64()),
        emotion_expression: con
            .and_then(|v| {
                v.get("emotion_expression")
                    .or_else(|| v.get("emotionExpression"))
            })
            .and_then(|v| v.as_f64()),
        narrative_purpose: con
            .and_then(|v| {
                v.get("narrative_purpose")
                    .or_else(|| v.get("narrativePurpose"))
            })
            .and_then(|v| v.as_f64()),
    }
}

fn parse_commercial_scores(parsed: &serde_json::Value) -> CommercialScores {
    let com = parsed
        .get("commercial_scores")
        .or_else(|| parsed.get("commercialScores"));
    CommercialScores {
        platform_fit: com
            .and_then(|v| v.get("platform_fit").or_else(|| v.get("platformFit")))
            .and_then(|v| v.as_f64()),
        audience_fit: com
            .and_then(|v| v.get("audience_fit").or_else(|| v.get("audienceFit")))
            .and_then(|v| v.as_f64()),
        conversion_potential: com
            .and_then(|v| {
                v.get("conversion_potential")
                    .or_else(|| v.get("conversionPotential"))
            })
            .and_then(|v| v.as_f64()),
    }
}

fn parse_technical_scores(parsed: &serde_json::Value) -> TechnicalScores {
    let tec = parsed
        .get("technical_scores")
        .or_else(|| parsed.get("technicalScores"));
    TechnicalScores {
        clarity: tec.and_then(|v| v.get("clarity")).and_then(|v| v.as_f64()),
        distortion: tec
            .and_then(|v| v.get("distortion"))
            .and_then(|v| v.as_f64()),
        character_consistency: tec
            .and_then(|v| {
                v.get("character_consistency")
                    .or_else(|| v.get("characterConsistency"))
            })
            .and_then(|v| v.as_f64()),
        motion_quality: tec
            .and_then(|v| v.get("motion_quality").or_else(|| v.get("motionQuality")))
            .and_then(|v| v.as_f64()),
    }
}

fn parse_issues(parsed: &serde_json::Value) -> Vec<ReviewIssue> {
    let Some(issues_arr) = parsed.get("issues").and_then(|v| v.as_array()) else {
        return vec![];
    };

    issues_arr
        .iter()
        .filter_map(|issue| {
            let dimension = issue.get("dimension")?.as_str()?.to_owned();
            let severity_str = issue.get("severity")?.as_str()?;
            let severity = IssueSeverity::parse(severity_str).ok()?;
            let message = issue.get("message")?.as_str()?.to_owned();
            let suggested_fix = issue
                .get("suggested_fix")
                .or_else(|| issue.get("suggestedFix"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();

            Some(ReviewIssue {
                dimension,
                severity,
                message,
                suggested_fix,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_vision_result() {
        let json = r#"{
            "requirement_scores": {"match": 85, "completeness": 80, "clarity": 75},
            "visual_scores": {"composition": 90, "color": 85, "lighting": 80, "texture": 75, "lens_language": 82},
            "content_scores": {"theme_match": 88, "emotion_expression": 85, "narrative_purpose": 80},
            "commercial_scores": {"platform_fit": 70, "audience_fit": 75, "conversion_potential": 65},
            "technical_scores": {"clarity": 90, "distortion": 85, "character_consistency": 88, "motion_quality": 82},
            "overall": 82.5,
            "decision": "accept_with_suggestions",
            "confidence": 0.85,
            "issues": [
                {"dimension": "character_consistency", "severity": "medium", "message": "角色脸部特征略有偏差", "suggested_fix": "强化角色参考图约束"}
            ]
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();

        let req = parse_requirement_scores(&parsed);
        assert_eq!(req.requirement_match, Some(85.0));

        let vis = parse_visual_scores(&parsed);
        assert_eq!(vis.composition, Some(90.0));

        let issues = parse_issues(&parsed);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].dimension, "character_consistency");
    }

    #[test]
    fn parse_camelcase_keys() {
        let json = r#"{
            "requirementScores": {"match": 80},
            "visualScores": {"composition": 85},
            "contentScores": {"themeMatch": 90},
            "commercialScores": {"platformFit": 75},
            "technicalScores": {"characterConsistency": 88},
            "issues": []
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
        let req = parse_requirement_scores(&parsed);
        assert_eq!(req.requirement_match, Some(80.0));

        let vis = parse_visual_scores(&parsed);
        assert_eq!(vis.composition, Some(85.0));
    }

    #[test]
    fn parse_empty_json() {
        let json = r#"{}"#;
        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();

        let req = parse_requirement_scores(&parsed);
        assert_eq!(req.requirement_match, None);

        let issues = parse_issues(&parsed);
        assert!(issues.is_empty());
    }
}
