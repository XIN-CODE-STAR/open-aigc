//! DashScope Qwen-VL Captioning 适配器。
//!
//! 调用阿里云 DashScope 的 Qwen-VL 视觉模型进行图片 captioning。
//! 使用 OpenAI 兼容的 /v1/chat/completions 端点。
//!
//! 支持模型：qwen-vl-max、qwen-vl-plus、qwen-vl-max-latest

use crate::domain::common::InferenceSource;
use crate::domain::semantic::{SemanticEntity, SemanticTag};
use crate::ports::captioning_port::{CaptionError, CaptionRequest, CaptionResult, CaptioningPort};

const DEFAULT_BASE_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";
// 注意：qwen-vl-max-latest 需在百炼控制台单独开通（未开通报 access_denied），
// 默认使用已验证可用的 qwen-vl-max。
const DEFAULT_MODEL: &str = "qwen-vl-max";

/// 分析 prompt — 面向"理解以编辑"的深度结构化分析。
/// 保持顶层 JSON 结构（caption/tags/entities/ocr_text）以兼容解析与存储，
/// 但对各字段提出覆盖版式、配色、文字转写的深度要求。
const ANALYSIS_PROMPT: &str = r#"你是专业的视觉分析师。请全面分析这张图片，以 JSON 格式输出（不要输出任何其他内容）：
{
  "caption": "全面的视觉分析（8-12 句），必须按顺序覆盖：1) 图片类型与用途（海报/插画/照片/UI截图/电商图等）；2) 版式布局（上中下、左右如何分区，每个区域放了什么元素，元素间的对齐与间距关系）；3) 视觉风格（写实/扁平/3D/手绘/国潮/赛博等，光线与质感）；4) 配色方案（背景色、主色、点缀色，尽量给出近似色值如「深蓝约 #1a2b4c」）；5) 核心主体及其细节特征；6) 特殊元素：二维码、水印、Logo、按钮、徽章等的位置与外观。描述必须具体到没看过图的人能准确复述版式",
  "tags": [
    {"name": "标签名（覆盖主题、风格、配色、用途、情绪，共 8-15 个）", "confidence": 0.9}
  ],
  "entities": [
    {"type": "character|object|location|style|action", "name": "实体名（图中所有可命名元素，含 Logo/二维码/按钮等）", "confidence": 0.9}
  ],
  "ocr_text": "完整转写图片中所有可见文字，逐字转写不要概括，按区域组织，每条格式为「文字内容（位置）」，用分号分隔。例如：新品发布（顶部居中）；限时 5 折（中部大字）；立即购买（按钮内）；https://example.com（底部小字）。没有文字则为 null"
}"#;

/// DashScope Qwen-VL Captioning 适配器。
pub struct DashScopeCaptioner {
    base_url: String,
    api_key: String,
    model: String,
    timeout_secs: u64,
}

impl DashScopeCaptioner {
    pub fn new(api_key: String) -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_owned(),
            api_key,
            model: DEFAULT_MODEL.to_owned(),
            timeout_secs: 120,
        }
    }

    #[allow(dead_code)]
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    #[allow(dead_code)]
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    #[allow(dead_code)]
    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout_secs = seconds;
        self
    }

    fn endpoint(&self) -> String {
        let trimmed = self.base_url.trim_end_matches('/');
        if trimmed.ends_with("/chat/completions") {
            trimmed.to_owned()
        } else if trimmed.ends_with("/v1") {
            format!("{trimmed}/chat/completions")
        } else {
            format!("{trimmed}/v1/chat/completions")
        }
    }

    fn build_request_body(&self, image_url: &str, instruction: Option<&str>) -> serde_json::Value {
        let prompt = instruction.unwrap_or(ANALYSIS_PROMPT);

        let image_content = if image_url.starts_with("data:") {
            // base64 data URL — 直接传给 OpenAI 兼容格式
            serde_json::json!({
                "type": "image_url",
                "image_url": { "url": image_url }
            })
        } else {
            // HTTP URL
            serde_json::json!({
                "type": "image_url",
                "image_url": { "url": image_url }
            })
        };

        serde_json::json!({
            "model": self.model,
            "max_tokens": 3000,
            "temperature": 0.1,
            "messages": [
                {
                    "role": "system",
                    "content": "你是一个专业的图片分析专家，擅长描述图片内容、识别物体和场景、提取文字。始终输出纯 JSON，不要包含 markdown 代码块标记。"
                },
                {
                    "role": "user",
                    "content": [
                        image_content,
                        {
                            "type": "text",
                            "text": prompt
                        }
                    ]
                }
            ]
        })
    }

    /// 解析模型响应中的 JSON，提取 caption、tags、entities、ocr_text。
    fn parse_response(content: &str) -> Result<ParsedCaption, CaptionError> {
        // 尝试提取 JSON（可能被 markdown 代码块包裹）
        let json_str = extract_json_from_text(content);

        let parsed: serde_json::Value = serde_json::from_str(&json_str).map_err(|e| {
            CaptionError::ParseError(format!(
                "failed to parse caption JSON: {e}; raw: {}",
                &content[..content.len().min(200)]
            ))
        })?;

        let caption = parsed
            .get("caption")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        if caption.is_empty() {
            return Err(CaptionError::ParseError(
                "caption field is empty or missing".to_owned(),
            ));
        }

        // 解析 tags
        let tags: Vec<SemanticTag> = parsed
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        let name = t.get("name")?.as_str()?.to_owned();
                        let confidence = t.get("confidence")?.as_f64()? as f32;
                        Some(SemanticTag {
                            name,
                            confidence: confidence.clamp(0.0, 1.0),
                            source: InferenceSource::Llm,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // 解析 entities
        let entities: Vec<SemanticEntity> = parsed
            .get("entities")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| {
                        let entity_type = e.get("type")?.as_str()?.to_owned();
                        let name = e.get("name")?.as_str()?.to_owned();
                        let confidence = e.get("confidence")?.as_f64()? as f32;
                        Some(SemanticEntity {
                            entity_type,
                            name,
                            confidence: confidence.clamp(0.0, 1.0),
                            bbox: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // 解析 ocr_text
        let ocr_text = parsed
            .get("ocr_text")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty() && *s != "null")
            .map(String::from);

        Ok(ParsedCaption {
            caption,
            tags,
            entities,
            ocr_text,
        })
    }
}

/// 从文本中提取 JSON 字符串（处理 markdown 代码块包裹的情况）。
fn extract_json_from_text(text: &str) -> String {
    let trimmed = text.trim();

    // 如果已经是 JSON 对象，直接返回
    if trimmed.starts_with('{') {
        return trimmed.to_owned();
    }

    // 尝试提取 ```json ... ``` 代码块
    if let Some(start) = trimmed.find("```json") {
        let rest = &trimmed[start + 7..];
        if let Some(end) = rest.find("```") {
            return rest[..end].trim().to_owned();
        }
    }

    // 尝试提取 ``` ... ``` 代码块
    if let Some(start) = trimmed.find("```") {
        let rest = &trimmed[start + 3..];
        // 跳过语言标识行
        let json_start = rest.find('\n').map(|i| i + 1).unwrap_or(0);
        if let Some(end) = rest[json_start..].find("```") {
            return rest[json_start..json_start + end].trim().to_owned();
        }
    }

    // 查找第一个 { 和最后一个 }
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                return trimmed[start..=end].to_owned();
            }
        }
    }

    trimmed.to_owned()
}

/// 解析后的 caption 数据。
struct ParsedCaption {
    caption: String,
    tags: Vec<SemanticTag>,
    entities: Vec<SemanticEntity>,
    ocr_text: Option<String>,
}

impl CaptioningPort for DashScopeCaptioner {
    fn adapter_id(&self) -> &str {
        "dashscope-qwen-vl"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec!["qwen-vl-max", "qwen-vl-plus", "qwen-vl-max-latest"]
    }

    fn is_ready(&self) -> bool {
        !self.api_key.trim().is_empty()
    }

    fn caption_image(&self, request: &CaptionRequest) -> Result<CaptionResult, CaptionError> {
        if self.api_key.trim().is_empty() {
            return Err(CaptionError::NotReady("missing API key".to_owned()));
        }

        let endpoint = self.endpoint();
        eprintln!(
            "[DashScopeCaptioner] POST {} model={} image_len={}",
            endpoint,
            self.model,
            request.image_url.len()
        );

        let body = self.build_request_body(&request.image_url, request.instruction.as_deref());

        let response = ureq::post(&endpoint)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|e| CaptionError::ApiError(format!("HTTP request failed: {e}")))?;

        let status = response.status();
        if status >= 400 {
            let body = response
                .into_string()
                .unwrap_or_else(|_| "unknown error".to_owned());
            return Err(CaptionError::ApiError(format!(
                "API returned status {status}: {body}"
            )));
        }

        let result: serde_json::Value = response
            .into_json()
            .map_err(|e| CaptionError::ParseError(format!("failed to parse API response: {e}")))?;

        // 提取 content text
        let content_text = result
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| {
                CaptionError::ParseError(
                    "missing choices[0].message.content in response".to_owned(),
                )
            })?;

        let tokens_used = result
            .get("usage")
            .and_then(|u| u.get("total_tokens"))
            .and_then(|t| t.as_u64())
            .map(|t| t as u32);

        eprintln!(
            "[DashScopeCaptioner] response len={} tokens={:?}",
            content_text.len(),
            tokens_used
        );

        let parsed = Self::parse_response(content_text)?;

        Ok(CaptionResult {
            caption: parsed.caption,
            tags: parsed.tags,
            entities: parsed.entities,
            ocr_text: parsed.ocr_text,
            raw_response: content_text.to_owned(),
            tokens_used,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_from_code_block() {
        let input = r#"Here is the analysis:
```json
{"caption": "A cat sitting on a table", "tags": [], "entities": [], "ocr_text": null}
```"#;
        let json = extract_json_from_text(input);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["caption"], "A cat sitting on a table");
    }

    #[test]
    fn extract_json_from_raw() {
        let input = r#"{"caption": "test", "tags": [], "entities": [], "ocr_text": null}"#;
        let json = extract_json_from_text(input);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["caption"], "test");
    }

    #[test]
    fn parse_full_response() {
        let content = r#"{
            "caption": "一名男子坐在咖啡馆中使用笔记本电脑",
            "tags": [
                {"name": "咖啡馆", "confidence": 0.95},
                {"name": "办公", "confidence": 0.88}
            ],
            "entities": [
                {"type": "character", "name": "男子", "confidence": 0.9},
                {"type": "object", "name": "笔记本电脑", "confidence": 0.95}
            ],
            "ocr_text": null
        }"#;

        let parsed = DashScopeCaptioner::parse_response(content).unwrap();
        assert!(parsed.caption.contains("咖啡馆"));
        assert_eq!(parsed.tags.len(), 2);
        assert_eq!(parsed.entities.len(), 2);
        assert!(parsed.ocr_text.is_none());
    }
}
