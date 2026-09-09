use crate::{application::error::AppError, ports::vision_adapter::VisionAdapter};

/// Content Guard LLM Prompt 模板。
pub struct ContentGuardPrompts;

impl ContentGuardPrompts {
    /// 内容安全检查 System Prompt。
    pub fn content_safety_system() -> &'static str {
        r#"
你是一个专业的内容安全审查员（Content Safety Reviewer）。

你的任务是对用户生成的内容（文本、Prompt、脚本）进行安全审查，识别潜在风险。

审查维度：
1. 政治敏感：涉及政治立场、意识形态、敏感历史事件
2. 色情暴力：涉及色情、暴力、血腥、恐怖内容
3. 仇恨言论：涉及种族、性别、宗教歧视
4. 违法内容：涉及毒品、赌博、诈骗等违法行为
5. 未成年人保护：涉及未成年人的不当内容
6. 医疗/金融/教育风险：涉及专业领域的误导性内容

输出格式（严格 JSON）：
{
  "status": "passed|needs_review|flagged|blocked",
  "risk_level": "low|medium|high|critical",
  "checks": [
    {
      "category": "political_sensitivity|nsfw_content|hate_speech|illegal_content|minor_protection|professional_risk",
      "status": "pass|warn|flag|block",
      "message": "具体问题描述",
      "detail": "详细说明",
      "suggestion": "修改建议"
    }
  ],
  "overall_assessment": "一句话总结",
  "confidence": 0.0-1.0
}

评分标准：
- passed：内容安全，可正常使用
- needs_review：存在潜在风险，建议人工复核
- flagged：存在明显风险，需要修改
- blocked：存在严重风险，禁止使用
"#
    }

    /// 品牌一致性检查 Prompt 模板。
    pub fn brand_consistency_system(brand_guidelines: &str) -> String {
        format!(
            r#"
你是一个专业的品牌一致性审查员（Brand Consistency Reviewer）。

品牌指南：
{brand_guidelines}

你的任务是检查内容是否符合品牌指南，包括：
1. 视觉风格：色彩、排版、构图是否符合品牌调性
2. 语言风格：用词、语气、表达方式是否符合品牌定位
3. 价值观：内容是否传达正确的品牌价值观
4. 合规性：是否包含品牌禁止的内容或表达

输出格式（严格 JSON）：
{{
  "brand_consistency_score": 0-100,
  "checks": [
    {{
      "dimension": "visual_style|language_style|values|compliance",
      "status": "pass|warn|flag|block",
      "message": "具体问题描述",
      "detail": "详细说明"
    }}
  ],
  "recommendations": ["修改建议1", "修改建议2"],
  "confidence": 0.0-1.0
}}
"#
        )
    }

    /// 平台合规检查 Prompt 模板。
    pub fn platform_compliance_system(platform: &str) -> String {
        format!(
            r#"
你是一个专业的平台合规审查员（Platform Compliance Reviewer）。

目标平台：{platform}

你的任务是检查内容是否符合目标平台的规范和要求：
1. 内容规范：平台的内容政策和社区准则
2. 格式要求：尺寸、时长、分辨率等技术要求
3. 商业规范：广告法、商标法等商业合规要求
4. 版权风险：可能的版权侵权风险

输出格式（严格 JSON）：
{{
  "platform_compliance_score": 0-100,
  "checks": [
    {{
      "category": "content_policy|format_requirements|commercial_compliance|copyright_risk",
      "status": "pass|warn|flag|block",
      "message": "具体问题描述",
      "detail": "详细说明",
      "platform_rule": "相关平台规则引用"
    }}
  ],
  "recommendations": ["修改建议1", "修改建议2"],
  "confidence": 0.0-1.0
}}
"#
        )
    }
}

/// LLM Content Guard：集成 LLM 的内容安全检查服务。
pub struct LlmContentGuard {
    vision_adapters: Vec<Box<dyn VisionAdapter>>,
    default_adapter_id: String,
    default_model: String,
}

impl LlmContentGuard {
    pub fn new(
        vision_adapters: Vec<Box<dyn VisionAdapter>>,
        default_adapter_id: String,
        default_model: String,
    ) -> Self {
        Self {
            vision_adapters,
            default_adapter_id,
            default_model,
        }
    }

    /// 使用 LLM 进行内容安全检查。
    pub fn check_content_safety(
        &self,
        content_text: &str,
        api_key: &str,
        adapter_id: Option<&str>,
    ) -> Result<ContentGuardCheckResult, AppError> {
        let adapter_id = adapter_id.unwrap_or(&self.default_adapter_id);
        let system_prompt = ContentGuardPrompts::content_safety_system();
        let user_prompt = format!("请对以下内容进行安全审查：\n\n{content_text}");

        let adapter = self
            .vision_adapters
            .iter()
            .find(|a| a.adapter_id() == adapter_id)
            .ok_or_else(|| {
                AppError::new(
                    "guard adapter not found",
                    std::io::Error::other(format!("adapter '{adapter_id}' not found")),
                )
            })?;

        let result = adapter.evaluate_image(
            "", // 无图片，纯文本
            system_prompt,
            &user_prompt,
            &self.default_model,
            api_key,
            None,
        )?;

        Self::parse_guard_result(&result.raw_json)
    }

    /// 使用 LLM 进行品牌一致性检查。
    pub fn check_brand_consistency(
        &self,
        content_text: &str,
        brand_guidelines: &str,
        api_key: &str,
    ) -> Result<ContentGuardCheckResult, AppError> {
        let system_prompt = ContentGuardPrompts::brand_consistency_system(brand_guidelines);
        let user_prompt = format!("请检查以下内容是否符合品牌指南：\n\n{content_text}");

        let adapter = self
            .vision_adapters
            .iter()
            .find(|a| a.adapter_id() == &self.default_adapter_id)
            .ok_or_else(|| {
                AppError::new(
                    "guard adapter not found",
                    std::io::Error::other("default adapter not found"),
                )
            })?;

        let result = adapter.evaluate_image(
            "",
            &system_prompt,
            &user_prompt,
            &self.default_model,
            api_key,
            None,
        )?;

        Self::parse_guard_result(&result.raw_json)
    }

    /// 使用 LLM 进行平台合规检查。
    pub fn check_platform_compliance(
        &self,
        content_text: &str,
        platform: &str,
        api_key: &str,
    ) -> Result<ContentGuardCheckResult, AppError> {
        let system_prompt = ContentGuardPrompts::platform_compliance_system(platform);
        let user_prompt = format!("请检查以下内容是否符合{platform}平台规范：\n\n{content_text}");

        let adapter = self
            .vision_adapters
            .iter()
            .find(|a| a.adapter_id() == &self.default_adapter_id)
            .ok_or_else(|| {
                AppError::new(
                    "guard adapter not found",
                    std::io::Error::other("default adapter not found"),
                )
            })?;

        let result = adapter.evaluate_image(
            "",
            &system_prompt,
            &user_prompt,
            &self.default_model,
            api_key,
            None,
        )?;

        Self::parse_guard_result(&result.raw_json)
    }

    /// 解析 LLM 返回的检查结果。
    fn parse_guard_result(json_str: &str) -> Result<ContentGuardCheckResult, AppError> {
        let parsed: serde_json::Value =
            serde_json::from_str(json_str).map_err(|e| AppError::new("parse guard result", e))?;

        let status = parsed
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("needs_review");

        let risk_level = parsed
            .get("risk_level")
            .and_then(|v| v.as_str())
            .unwrap_or("medium");

        let checks = parsed
            .get("checks")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let confidence = parsed
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.7);

        Ok(ContentGuardCheckResult {
            status: status.to_owned(),
            risk_level: risk_level.to_owned(),
            checks,
            confidence,
            raw_json: json_str.to_owned(),
        })
    }
}

/// Content Guard 检查结果。
#[derive(Debug, Clone)]
pub struct ContentGuardCheckResult {
    pub status: String,
    pub risk_level: String,
    pub checks: Vec<serde_json::Value>,
    pub confidence: f64,
    pub raw_json: String,
}

/// 增强版 Content Guard 规则引擎。
pub struct EnhancedContentGuardRules;

impl EnhancedContentGuardRules {
    /// 多语言关键词检查（中英文）。
    pub fn check_multilingual_keywords(text: &str) -> Vec<serde_json::Value> {
        let mut checks = Vec::new();
        let text_lower = text.to_lowercase();

        // 中文政治敏感词
        let political_zh = [
            "革命", "推翻", "暴力", "恐怖", "独裁", "分裂", "颠覆", "叛国", "邪教",
        ];
        // 英文政治敏感词
        let political_en = [
            "revolution",
            "overthrow",
            "terrorism",
            "dictator",
            "separatist",
            "treason",
        ];

        let hit_political_zh: Vec<_> = political_zh
            .iter()
            .filter(|kw| text.contains(**kw))
            .collect();
        let hit_political_en: Vec<_> = political_en
            .iter()
            .filter(|kw| text_lower.contains(**kw))
            .collect();

        if !hit_political_zh.is_empty() || !hit_political_en.is_empty() {
            let mut all_hits = hit_political_zh
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>();
            all_hits.extend(hit_political_en.iter().map(|s| s.to_string()));
            checks.push(serde_json::json!({
                "category": "political_sensitivity",
                "status": "block",
                "message": format!("检测到敏感政治关键词：{}", all_hits.join(", ")),
                "detail": "内容包含需要人工复核的表述，已自动阻断。"
            }));
        }

        // 中文色情暴力词
        let nsfw_zh = ["色情", "暴力", "血腥", "裸露", "毒品", "赌博"];
        // 英文色情暴力词
        let nsfw_en = ["porn", "violence", "gore", "nude", "drug", "gambling"];

        let hit_nsfw_zh: Vec<_> = nsfw_zh.iter().filter(|kw| text.contains(**kw)).collect();
        let hit_nsfw_en: Vec<_> = nsfw_en
            .iter()
            .filter(|kw| text_lower.contains(**kw))
            .collect();

        if !hit_nsfw_zh.is_empty() || !hit_nsfw_en.is_empty() {
            let mut all_hits = hit_nsfw_zh
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>();
            all_hits.extend(hit_nsfw_en.iter().map(|s| s.to_string()));
            checks.push(serde_json::json!({
                "category": "nsfw_content",
                "status": "flag",
                "message": format!("检测到敏感内容关键词：{}", all_hits.join(", ")),
                "detail": "内容可能不符合平台安全策略，已标记。"
            }));
        }

        // 中文仇恨言论
        let hate_zh = ["歧视", "侮辱", "仇恨", "种族主义", "性别歧视"];
        // 英文仇恨言论
        let hate_en = ["discriminate", "insult", "hate", "racism", "sexism"];

        let hit_hate_zh: Vec<_> = hate_zh.iter().filter(|kw| text.contains(**kw)).collect();
        let hit_hate_en: Vec<_> = hate_en
            .iter()
            .filter(|kw| text_lower.contains(**kw))
            .collect();

        if !hit_hate_zh.is_empty() || !hit_hate_en.is_empty() {
            let mut all_hits = hit_hate_zh
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>();
            all_hits.extend(hit_hate_en.iter().map(|s| s.to_string()));
            checks.push(serde_json::json!({
                "category": "hate_speech",
                "status": "flag",
                "message": format!("检测到仇恨言论关键词：{}", all_hits.join(", ")),
                "detail": "内容可能包含歧视性表述，已标记。"
            }));
        }

        checks
    }

    /// 平台特定检查。
    pub fn check_platform_specific(text: &str, platform: &str) -> Vec<serde_json::Value> {
        let mut checks = Vec::new();

        match platform {
            "douyin" | "tiktok" => {
                // 抖音/TikTok 特殊规则
                if text.contains("微信") || text.contains("WeChat") {
                    checks.push(serde_json::json!({
                        "category": "platform_compliance",
                        "status": "warn",
                        "message": "抖音平台不建议提及微信相关内容",
                        "detail": "抖音与微信存在竞争关系，提及微信可能影响内容分发。",
                        "platform_rule": "抖音社区规范"
                    }));
                }
            }
            "wechat" => {
                // 微信公众号特殊规则
                if text.len() > 20000 {
                    checks.push(serde_json::json!({
                        "category": "format_requirements",
                        "status": "warn",
                        "message": "文章长度超过微信公众号建议限制",
                        "detail": "微信公众号文章建议控制在20000字以内，过长可能影响阅读体验。",
                        "platform_rule": "微信公众号运营规范"
                    }));
                }
            }
            "xiaohongshu" | "red" => {
                // 小红书特殊规则
                if text.contains("广告") || text.contains("推广") {
                    checks.push(serde_json::json!({
                        "category": "commercial_compliance",
                        "status": "warn",
                        "message": "小红书平台对广告内容有特殊标注要求",
                        "detail": "小红书要求广告内容必须明确标注，否则可能被限流或删除。",
                        "platform_rule": "小红书社区规范"
                    }));
                }
            }
            _ => {}
        }

        checks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multilingual_keywords_detects_chinese() {
        let text = "这个内容包含色情和血腥元素";
        let checks = EnhancedContentGuardRules::check_multilingual_keywords(text);
        assert!(!checks.is_empty());
        assert_eq!(checks[0]["category"], "nsfw_content");
    }

    #[test]
    fn multilingual_keywords_detects_english() {
        let text = "This content contains violence and gore";
        let checks = EnhancedContentGuardRules::check_multilingual_keywords(text);
        assert!(!checks.is_empty());
    }

    #[test]
    fn platform_specific_douyin() {
        let text = "关注我的微信公众号";
        let checks = EnhancedContentGuardRules::check_platform_specific(text, "douyin");
        assert!(!checks.is_empty());
        assert_eq!(checks[0]["category"], "platform_compliance");
    }

    #[test]
    fn platform_specific_xiaohongshu() {
        let text = "这是一个广告推广内容";
        let checks = EnhancedContentGuardRules::check_platform_specific(text, "xiaohongshu");
        assert!(!checks.is_empty());
    }

    #[test]
    fn clean_content_passes() {
        let text = "这是一个关于创业的纪录片脚本";
        let checks = EnhancedContentGuardRules::check_multilingual_keywords(text);
        assert!(checks.is_empty());
    }
}
