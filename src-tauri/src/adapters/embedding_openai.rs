//! OpenAI 兼容文本嵌入适配器。
//!
//! 实现 `EmbeddingPort`：POST `{base_url}/embeddings`（OpenAI 兼容格式，
//! GLM / xAI / OpenAI 等供应商均适用）。base_url 直接使用凭据里存的值
//! （例如 `https://open.bigmodel.cn/api/paas/v4`）。

use crate::ports::embedding_port::{
    EmbeddingAdapterInfo, EmbeddingError, EmbeddingPort, EmbeddingRequest, EmbeddingResult,
};

pub struct OpenAiCompatibleEmbeddingAdapter {
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAiCompatibleEmbeddingAdapter {
    pub fn new(base_url: String, api_key: String, model: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            api_key,
            model,
        }
    }
}

impl EmbeddingPort for OpenAiCompatibleEmbeddingAdapter {
    fn info(&self) -> EmbeddingAdapterInfo {
        EmbeddingAdapterInfo {
            adapter_id: "openai_compatible_embedding".to_owned(),
            name: "OpenAI 兼容文本嵌入".to_owned(),
            ready: self.is_ready(),
            supported_models: vec![self.model.clone()],
            // 实际维度由响应决定；这里仅是声明值
            dimension: 0,
        }
    }

    fn adapter_id(&self) -> &str {
        "openai_compatible_embedding"
    }

    fn is_ready(&self) -> bool {
        !self.base_url.is_empty() && !self.api_key.is_empty()
    }

    fn dimension(&self) -> usize {
        0
    }

    fn embed(&self, request: &EmbeddingRequest) -> Result<EmbeddingResult, EmbeddingError> {
        let results = self.embed_batch(std::slice::from_ref(request))?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| EmbeddingError::Internal("empty embedding response".to_owned()))
    }

    fn embed_batch(
        &self,
        requests: &[EmbeddingRequest],
    ) -> Result<Vec<EmbeddingResult>, EmbeddingError> {
        if !self.is_ready() {
            return Err(EmbeddingError::NotReady(
                "嵌入凭据未配置（需要 base_url 与 API key）".to_owned(),
            ));
        }
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        let url = format!("{}/embeddings", self.base_url);
        let inputs: Vec<&str> = requests.iter().map(|r| r.text.as_str()).collect();
        let response = ureq::post(&url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(30))
            .send_json(serde_json::json!({ "model": self.model, "input": inputs }))
            .map_err(|e| {
                EmbeddingError::EmbeddingFailed(format!("embedding request failed: {e}"))
            })?;

        let status = response.status();
        let body = response.into_string().map_err(|e| {
            EmbeddingError::EmbeddingFailed(format!("read embedding response: {e}"))
        })?;
        if status >= 400 {
            let snippet: String = body.chars().take(300).collect();
            return Err(EmbeddingError::EmbeddingFailed(format!(
                "embedding endpoint returned {status}: {snippet}"
            )));
        }
        parse_embeddings_response(&body)
    }
}

/// 解析 OpenAI 兼容 /embeddings 响应：`data: [{index, embedding}]` 按 index 对齐输入。
fn parse_embeddings_response(body: &str) -> Result<Vec<EmbeddingResult>, EmbeddingError> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| EmbeddingError::EmbeddingFailed(e.to_string()))?;
    let model = value
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_owned();
    let data = value
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| EmbeddingError::Internal("missing data array".to_owned()))?;

    let mut results: Vec<Option<EmbeddingResult>> = vec![None; data.len()];
    for item in data {
        let index = item.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let embedding: Vec<f32> = item
            .get("embedding")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_f64().map(|f| f as f32))
                    .collect()
            })
            .ok_or_else(|| EmbeddingError::Internal("missing embedding vector".to_owned()))?;
        if index >= results.len() {
            return Err(EmbeddingError::Internal(
                "embedding index out of range".to_owned(),
            ));
        }
        results[index] = Some(EmbeddingResult {
            embedding,
            model: model.clone(),
            tokens_used: None,
        });
    }
    results
        .into_iter()
        .enumerate()
        .map(|(i, slot)| {
            slot.ok_or_else(|| EmbeddingError::Internal(format!("missing embedding at index {i}")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_aligns_embeddings_by_index() {
        let body = r#"{
            "model": "embedding-3",
            "data": [
                {"index": 1, "embedding": [0.4, 0.5]},
                {"index": 0, "embedding": [0.1, 0.2, 0.3]}
            ]
        }"#;
        let results = parse_embeddings_response(body).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].embedding, vec![0.1, 0.2, 0.3]);
        assert_eq!(results[1].embedding, vec![0.4, 0.5]);
        assert_eq!(results[0].model, "embedding-3");
    }

    #[test]
    fn parse_rejects_missing_vector() {
        let body = r#"{"data": [{"index": 0}]}"#;
        assert!(parse_embeddings_response(body).is_err());
    }

    /// 手动冒烟测试：用真实工作区凭据验证 /embeddings 端点可用性。
    /// 运行：cargo test --lib manual_embedding_smoke -- --ignored --nocapture
    /// 任何失败都意味着当前凭据不支持嵌入 → 检索自动降级为关键词（功能无损）。
    #[test]
    #[ignore = "manual smoke test against real credential"]
    fn manual_embedding_smoke() {
        use crate::adapters::sqlite::credential_repository::SqliteCredentialRepository;
        use crate::ports::credential_repository::CredentialRepository;

        let workspace = std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap())
            .join("AppData")
            .join("Local")
            .join("com.aigcstudio.desktop")
            .join("workspace");
        let db = workspace.join("aigc-studio.sqlite3");
        let mut repo = SqliteCredentialRepository::open(&db).expect("open credential repo");
        let all = repo.list().expect("list credentials");
        let candidates: Vec<_> = all
            .iter()
            .filter(|c| c.enabled && !c.base_url.is_empty())
            .collect();
        eprintln!("enabled credentials: {}", candidates.len());
        let mut succeeded = false;
        // 按供应商尝试常见嵌入模型名
        let model_candidates = [
            "embedding-3",
            "text-embedding-v3",
            "text-embedding-v2",
            "text-embedding-v1",
        ];
        for credential in candidates.iter().rev() {
            let Ok(api_key) = repo.get_secret(&credential.credential_key) else {
                continue;
            };
            if api_key.is_empty() {
                continue;
            }
            for model in model_candidates {
                let adapter = OpenAiCompatibleEmbeddingAdapter::new(
                    credential.base_url.clone(),
                    api_key.clone(),
                    model.to_owned(),
                );
                eprintln!(
                    "trying provider={} base_url={} model={model} ...",
                    credential.provider_name, credential.base_url
                );
                match adapter.embed(&EmbeddingRequest {
                    text: "城市夜景 赛博朋克".to_owned(),
                    model: None,
                    asset_id: None,
                }) {
                    Ok(result) => {
                        eprintln!(
                            "SUCCESS provider={} model={} dims={}",
                            credential.provider_name,
                            result.model,
                            result.embedding.len()
                        );
                        succeeded = true;
                        break;
                    }
                    Err(e) => eprintln!("FAILED: {e}"),
                }
            }
            if succeeded {
                break;
            }
        }
        assert!(succeeded, "没有任何凭据的 /embeddings 端点可用");
    }

    #[test]
    fn not_ready_without_credentials() {
        let adapter = OpenAiCompatibleEmbeddingAdapter::new(
            String::new(),
            String::new(),
            "embedding-3".to_owned(),
        );
        assert!(!adapter.is_ready());
        let error = adapter
            .embed(&EmbeddingRequest {
                text: "hi".to_owned(),
                model: None,
                asset_id: None,
            })
            .unwrap_err();
        assert!(matches!(error, EmbeddingError::NotReady(_)));
    }
}
