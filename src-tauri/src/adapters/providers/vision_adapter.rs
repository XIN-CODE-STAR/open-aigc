use crate::{
    domain::providers::ProviderError,
    ports::vision_adapter::{VisionAdapter, VisionEvaluationResult},
};

/// Claude Vision 适配器。通过 Anthropic Messages API 调用 Claude 的视觉能力。
pub struct ClaudeVisionAdapter;

impl ClaudeVisionAdapter {
    pub fn new() -> Self {
        Self
    }

    fn default_base_url() -> &'static str {
        "https://api.anthropic.com"
    }

    fn build_messages_payload(
        image_url_or_base64: &str,
        system_prompt: &str,
        user_prompt: &str,
        model: &str,
    ) -> serde_json::Value {
        let image_content = if image_url_or_base64.starts_with("data:") {
            // base64 编码的图片
            serde_json::json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": "image/png",
                    "data": image_url_or_base64.split(',').nth(1).unwrap_or(image_url_or_base64)
                }
            })
        } else {
            // URL 图片
            serde_json::json!({
                "type": "image",
                "source": {
                    "type": "url",
                    "url": image_url_or_base64
                }
            })
        };

        serde_json::json!({
            "model": model,
            "max_tokens": 4096,
            "system": system_prompt,
            "messages": [{
                "role": "user",
                "content": [
                    image_content,
                    {
                        "type": "text",
                        "text": user_prompt
                    }
                ]
            }]
        })
    }

    fn extract_score_from_response(content: &str) -> Result<(f64, String, f64), ProviderError> {
        // 尝试从 JSON 响应中提取评分
        let parsed: serde_json::Value = serde_json::from_str(content)
            .map_err(|e| ProviderError::MalformedResponse(format!("JSON parse error: {e}")))?;

        let overall_score = parsed
            .get("overall")
            .or_else(|| parsed.get("overall_score"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let decision = parsed
            .get("decision")
            .and_then(|v| v.as_str())
            .unwrap_or("revise")
            .to_owned();

        let confidence = parsed
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.7);

        Ok((overall_score, decision, confidence))
    }
}

impl Default for ClaudeVisionAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl VisionAdapter for ClaudeVisionAdapter {
    fn adapter_id(&self) -> &str {
        "claude-vision"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec![
            "claude-sonnet-4-20250514",
            "claude-haiku-4-20250514",
            "claude-3-5-sonnet-20241022",
            "claude-3-5-haiku-20241022",
        ]
    }

    fn evaluate_image(
        &self,
        image_url_or_base64: &str,
        system_prompt: &str,
        user_prompt: &str,
        model: &str,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<VisionEvaluationResult, ProviderError> {
        let url = format!(
            "{}/v1/messages",
            base_url
                .unwrap_or(Self::default_base_url())
                .trim_end_matches('/')
        );

        let body =
            Self::build_messages_payload(image_url_or_base64, system_prompt, user_prompt, model);

        let response = ureq::post(&url)
            .set("x-api-key", api_key)
            .set("anthropic-version", "2023-06-01")
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let status = response.status();
        if status >= 400 {
            let body = response
                .into_string()
                .unwrap_or_else(|_| String::from("unknown"));
            return Err(ProviderError::Remote { status, body });
        }

        let result: serde_json::Value = response
            .into_json()
            .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

        // 提取 content text
        let content_text = result
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|block| block.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("");

        let tokens_used = result
            .get("usage")
            .and_then(|u| u.get("output_tokens"))
            .and_then(|t| t.as_u64())
            .map(|t| t as u32);

        let (overall_score, decision, confidence) =
            Self::extract_score_from_response(content_text)?;

        Ok(VisionEvaluationResult {
            raw_json: content_text.to_owned(),
            overall_score,
            decision,
            confidence,
            tokens_used,
            shot_id: None,
            asset_id: None,
        })
    }
}

/// GPT Vision 适配器。通过 OpenAI Chat Completions API 调用 GPT-4 Vision。
pub struct GptVisionAdapter;

impl GptVisionAdapter {
    pub fn new() -> Self {
        Self
    }

    fn default_base_url() -> &'static str {
        "https://api.openai.com"
    }

    fn build_messages_payload(
        image_url_or_base64: &str,
        system_prompt: &str,
        user_prompt: &str,
        model: &str,
    ) -> serde_json::Value {
        let image_url = image_url_or_base64.to_owned();

        serde_json::json!({
            "model": model,
            "max_tokens": 4096,
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt
                },
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "image_url",
                            "image_url": {
                                "url": image_url,
                                "detail": "high"
                            }
                        },
                        {
                            "type": "text",
                            "text": user_prompt
                        }
                    ]
                }
            ]
        })
    }
}

impl Default for GptVisionAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl VisionAdapter for GptVisionAdapter {
    fn adapter_id(&self) -> &str {
        "gpt-vision"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec![
            "gpt-4o",
            "gpt-4o-mini",
            "gpt-4-turbo",
            "gpt-4-vision-preview",
        ]
    }

    fn evaluate_image(
        &self,
        image_url_or_base64: &str,
        system_prompt: &str,
        user_prompt: &str,
        model: &str,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<VisionEvaluationResult, ProviderError> {
        let url = format!(
            "{}/v1/chat/completions",
            base_url
                .unwrap_or(Self::default_base_url())
                .trim_end_matches('/')
        );

        let body =
            Self::build_messages_payload(image_url_or_base64, system_prompt, user_prompt, model);

        let response = ureq::post(&url)
            .set("Authorization", &format!("Bearer {api_key}"))
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let status = response.status();
        if status >= 400 {
            let body = response
                .into_string()
                .unwrap_or_else(|_| String::from("unknown"));
            return Err(ProviderError::Remote { status, body });
        }

        let result: serde_json::Value = response
            .into_json()
            .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

        let content_text = result
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
            .and_then(|t| t.as_str())
            .unwrap_or("");

        let tokens_used = result
            .get("usage")
            .and_then(|u| u.get("completion_tokens"))
            .and_then(|t| t.as_u64())
            .map(|t| t as u32);

        // 尝试提取 JSON 块（GPT 可能包裹在 ```json ... ``` 中）
        let json_str = if let Some(start) = content_text.find('{') {
            if let Some(end) = content_text.rfind('}') {
                &content_text[start..=end]
            } else {
                content_text
            }
        } else {
            content_text
        };

        let (overall_score, decision, confidence) =
            ClaudeVisionAdapter::extract_score_from_response(json_str)?;

        Ok(VisionEvaluationResult {
            raw_json: json_str.to_owned(),
            overall_score,
            decision,
            confidence,
            tokens_used,
            shot_id: None,
            asset_id: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_adapter_id() {
        let adapter = ClaudeVisionAdapter::new();
        assert_eq!(adapter.adapter_id(), "claude-vision");
        assert!(!adapter.supported_models().is_empty());
    }

    #[test]
    fn gpt_adapter_id() {
        let adapter = GptVisionAdapter::new();
        assert_eq!(adapter.adapter_id(), "gpt-vision");
        assert!(!adapter.supported_models().is_empty());
    }

    #[test]
    fn extract_score_from_valid_json() {
        let json =
            r#"{"overall": 85.0, "decision": "accept_with_suggestions", "confidence": 0.88}"#;
        let (score, decision, confidence) =
            ClaudeVisionAdapter::extract_score_from_response(json).unwrap();
        assert_eq!(score, 85.0);
        assert_eq!(decision, "accept_with_suggestions");
        assert!((confidence - 0.88).abs() < 0.01);
    }

    #[test]
    fn extract_score_from_json_with_overall_score_key() {
        let json = r#"{"overall_score": 72.5, "decision": "revise", "confidence": 0.75}"#;
        let (score, decision, _) = ClaudeVisionAdapter::extract_score_from_response(json).unwrap();
        assert_eq!(score, 72.5);
        assert_eq!(decision, "revise");
    }

    #[test]
    fn extract_score_handles_missing_fields() {
        let json = r#"{"some_other_field": 42}"#;
        let (score, decision, confidence) =
            ClaudeVisionAdapter::extract_score_from_response(json).unwrap();
        assert_eq!(score, 0.0);
        assert_eq!(decision, "revise");
        assert_eq!(confidence, 0.7);
    }

    #[test]
    fn extract_score_from_markdown_wrapped_json() {
        let content = "Here is the evaluation:\n```json\n{\"overall\": 90, \"decision\": \"accept\", \"confidence\": 0.92}\n```";
        let json_str = if let Some(start) = content.find('{') {
            if let Some(end) = content.rfind('}') {
                &content[start..=end]
            } else {
                content
            }
        } else {
            content
        };
        let (score, decision, confidence) =
            ClaudeVisionAdapter::extract_score_from_response(json_str).unwrap();
        assert_eq!(score, 90.0);
        assert_eq!(decision, "accept");
        assert!((confidence - 0.92).abs() < 0.01);
    }
}
