use std::path::Path;

use crate::{
    domain::providers::{
        GenerationRequest, ProviderError, ProviderPollResult, ProviderSubmitResult,
    },
    ports::provider_adapter::ProviderAdapter,
};

/// OpenAI 兼容 API 适配器。适用于任何提供 /v1/generations 接口的供应商。
/// 各供应商可在此基础上实现具体的 SDK 调用逻辑。
pub struct OpenAiCompatibleAdapter;

impl OpenAiCompatibleAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for OpenAiCompatibleAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderAdapter for OpenAiCompatibleAdapter {
    fn provider_id(&self) -> &str {
        "openai_compatible"
    }

    fn submit(
        &mut self,
        base_url: &str,
        api_key: &str,
        model: &str,
        request: &GenerationRequest,
    ) -> Result<ProviderSubmitResult, ProviderError> {
        let url = format!("{}/v1/generations", base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": model,
            "prompt": request.prompt,
            "parameters": request.parameters,
        });
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
        let remote_task_id = result
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ProviderError::MalformedResponse("missing id field".to_owned()))?
            .to_owned();
        let remote_status = result
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("pending")
            .to_owned();
        Ok(ProviderSubmitResult {
            remote_task_id,
            status: remote_status,
        })
    }

    fn poll(
        &mut self,
        base_url: &str,
        api_key: &str,
        remote_task_id: &str,
    ) -> Result<ProviderPollResult, ProviderError> {
        let url = format!(
            "{}/v1/generations/{}",
            base_url.trim_end_matches('/'),
            remote_task_id
        );
        let response = ureq::get(&url)
            .set("Authorization", &format!("Bearer {api_key}"))
            .call()
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let status_code = response.status();
        if status_code >= 400 {
            let body = response.into_string().unwrap_or_default();
            return Err(ProviderError::Remote {
                status: status_code,
                body,
            });
        }
        let result: serde_json::Value = response
            .into_json()
            .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;
        let status = result
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("pending")
            .to_owned();
        let progress = result.get("progress").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
        let result_url = result
            .get("result_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned());
        let error_message = result
            .get("error")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned());
        Ok(ProviderPollResult {
            status,
            progress,
            result_url,
            error_message,
        })
    }

    fn download_result(
        &mut self,
        _base_url: &str,
        api_key: &str,
        result_url: &str,
        local_path: &Path,
    ) -> Result<(), ProviderError> {
        let response = ureq::get(result_url)
            .set("Authorization", &format!("Bearer {api_key}"))
            .call()
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let status = response.status();
        if status >= 400 {
            let body = response.into_string().unwrap_or_default();
            return Err(ProviderError::Remote { status, body });
        }
        let mut file = std::fs::File::create(local_path)
            .map_err(|e| ProviderError::Network(format!("failed to create local file: {e}")))?;
        let mut reader = response.into_reader();
        std::io::copy(&mut reader, &mut file)
            .map_err(|e| ProviderError::Network(format!("failed to write file: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_adapter() {
        let _adapter = OpenAiCompatibleAdapter::new();
    }
}
