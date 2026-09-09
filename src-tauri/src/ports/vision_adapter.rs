use crate::domain::providers::ProviderError;

/// LLM Vision API 端口。用于 AI Critic Agent 的多维评分。
///
/// 与 ProviderAdapter（生成型）不同，VisionAdapter 是分析型：
/// 接收图片 URL 或 base64，返回结构化评分。
#[allow(dead_code)]
pub trait VisionAdapter: Send {
    /// 适配器标识（如 "claude-vision" / "gpt-vision"）。
    fn adapter_id(&self) -> &str;

    /// 支持的模型列表。
    fn supported_models(&self) -> Vec<&str>;

    /// 对图片进行多维评价。
    ///
    /// 参数：
    /// - `image_url_or_base64`: 图片 URL 或 base64 编码
    /// - `system_prompt`: 评测 System Prompt（使用 CriticPromptTemplates 生成）
    /// - `user_prompt`: 用户需求/上下文描述
    /// - `model`: 使用的模型名称
    /// - `api_key`: API 密钥
    /// - `base_url`: API 基础 URL（可选，默认使用 Provider 默认地址）
    ///
    /// 返回：结构化 JSON 字符串（符合 ReviewReportRecord 的评分字段格式）
    fn evaluate_image(
        &self,
        image_url_or_base64: &str,
        system_prompt: &str,
        user_prompt: &str,
        model: &str,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<VisionEvaluationResult, ProviderError>;

    /// 对图片列表进行批量评价（用于分镜/关键帧批量评分）。
    fn evaluate_images_batch(
        &self,
        images: &[VisionImageInput],
        system_prompt: &str,
        user_prompt: &str,
        model: &str,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<Vec<VisionEvaluationResult>, ProviderError> {
        // 默认实现：逐张调用
        let mut results = Vec::with_capacity(images.len());
        for image in images {
            let result = self.evaluate_image(
                &image.url_or_base64,
                system_prompt,
                user_prompt,
                model,
                api_key,
                base_url,
            )?;
            results.push(result);
        }
        Ok(results)
    }
}

/// Vision API 图片输入。
#[derive(Debug, Clone)]
pub struct VisionImageInput {
    /// 图片 URL 或 base64 编码。
    pub url_or_base64: String,
    /// 可选的图片描述（用于上下文增强）。
    pub description: Option<String>,
    /// 关联的 shot_id（用于结果追溯）。
    pub shot_id: Option<String>,
    /// 关联的 asset_id（用于结果追溯）。
    pub asset_id: Option<String>,
}

/// Vision API 评价结果。
#[derive(Debug, Clone)]
pub struct VisionEvaluationResult {
    /// 原始 JSON 响应（符合五层评分结构）。
    pub raw_json: String,
    /// 解析后的综合评分。
    pub overall_score: f64,
    /// 解析后的评价决策。
    pub decision: String,
    /// 置信度。
    pub confidence: f64,
    /// 使用的 token 数量。
    pub tokens_used: Option<u32>,
    /// 关联的 shot_id（批量评价时用于追溯）。
    pub shot_id: Option<String>,
    /// 关联的 asset_id（批量评价时用于追溯）。
    pub asset_id: Option<String>,
}
