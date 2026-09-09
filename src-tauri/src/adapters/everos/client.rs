//! EverOS HTTP 客户端。
#![allow(dead_code)]
//!
//! 使用 `ureq` 调用 EverOS REST API，遵循项目中 `grok_chat.rs` 的 AgentBuilder 模式。

use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::EverosError;

/// EverOS HTTP 客户端。
pub struct EverosClient {
    agent: ureq::Agent,
    base_url: String,
}

/// EverOS 消息（发送给 /api/v1/memory/add）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EverosMessage {
    pub sender_id: String,
    pub role: String,
    pub timestamp: i64,
    pub content: String,
}

/// 搜索方法。
#[derive(Debug, Clone, Copy)]
pub enum SearchMethod {
    Keyword,
    Vector,
    Hybrid,
}

impl SearchMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Keyword => "keyword",
            Self::Vector => "vector",
            Self::Hybrid => "hybrid",
        }
    }
}

/// /api/v1/memory/add 请求体。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AddRequest<'a> {
    session_id: &'a str,
    app_id: &'a str,
    project_id: &'a str,
    messages: &'a [EverosMessage],
}

/// /api/v1/memory/add 响应数据。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddResult {
    pub message_count: u32,
    pub status: String,
}

/// /api/v1/memory/search 请求体。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchRequest<'a> {
    user_id: &'a str,
    query: &'a str,
    method: &'a str,
    top_k: i32,
    app_id: &'a str,
    project_id: &'a str,
}

/// 搜索结果中的原子事实。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactResult {
    pub id: Option<String>,
    pub content: String,
    pub score: f64,
}

/// 搜索结果中的对话片段。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeResult {
    pub id: String,
    pub summary: Option<String>,
    pub episode: Option<String>,
    pub score: f64,
    #[serde(default)]
    pub atomic_facts: Vec<FactResult>,
}

/// 搜索结果中的用户画像。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileResult {
    pub id: String,
    pub profile_data: Option<serde_json::Value>,
    pub score: Option<f64>,
}

/// 搜索结果中的 Agent 案例。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCaseResult {
    pub id: String,
    pub task_intent: String,
    pub approach: String,
    pub quality_score: f64,
    pub key_insight: Option<String>,
    pub score: f64,
}

/// /api/v1/memory/search 响应数据。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    #[serde(default)]
    pub episodes: Vec<EpisodeResult>,
    #[serde(default)]
    pub profiles: Vec<ProfileResult>,
    #[serde(default)]
    pub agent_cases: Vec<AgentCaseResult>,
    #[serde(default)]
    pub atomic_facts: Vec<FactResult>,
}

/// /api/v1/memory/flush 响应数据。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlushResult {
    pub status: String,
}

/// EverOS API 通用成功信封。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SuccessEnvelope<T> {
    #[allow(dead_code)]
    pub request_id: String,
    pub data: T,
}

/// /health 响应。
#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: String,
}

impl EverosClient {
    /// 创建新客户端。
    pub fn new(port: u16) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(30))
            .build();
        Self {
            agent,
            base_url: format!("http://127.0.0.1:{port}"),
        }
    }

    /// 健康检查。
    pub fn health(&self) -> Result<bool, EverosError> {
        let url = format!("{}/health", self.base_url);
        match self.agent.get(&url).call() {
            Ok(resp) => {
                let body: HealthResponse = resp
                    .into_json()
                    .map_err(|e| EverosError::Api(format!("health response parse: {e}")))?;
                Ok(body.status == "ok")
            }
            Err(ureq::Error::Status(503, _)) => Ok(false),
            Err(e) => Err(EverosError::Network(format!("health check: {e}"))),
        }
    }

    /// 存储对话消息。
    pub fn add_messages(
        &self,
        session_id: &str,
        messages: &[EverosMessage],
    ) -> Result<AddResult, EverosError> {
        let url = format!("{}/api/v1/memory/add", self.base_url);
        let body = AddRequest {
            session_id,
            app_id: "aigc-studio",
            project_id: "default",
            messages,
        };
        let resp: SuccessEnvelope<AddResult> = self
            .agent
            .post(&url)
            .send_json(&body)
            .map_err(|e| EverosError::Network(format!("add messages: {e}")))?
            .into_json()
            .map_err(|e| EverosError::Api(format!("add response parse: {e}")))?;
        Ok(resp.data)
    }

    /// 强制提取记忆。
    pub fn flush(&self, session_id: &str) -> Result<FlushResult, EverosError> {
        let url = format!("{}/api/v1/memory/flush", self.base_url);
        let body = serde_json::json!({
            "sessionId": session_id,
            "appId": "aigc-studio",
            "projectId": "default",
        });
        let resp: SuccessEnvelope<FlushResult> = self
            .agent
            .post(&url)
            .send_json(&body)
            .map_err(|e| EverosError::Network(format!("flush: {e}")))?
            .into_json()
            .map_err(|e| EverosError::Api(format!("flush response parse: {e}")))?;
        Ok(resp.data)
    }

    /// 检索相关记忆。
    pub fn search(
        &self,
        query: &str,
        user_id: &str,
        method: SearchMethod,
        top_k: i32,
    ) -> Result<SearchResult, EverosError> {
        let url = format!("{}/api/v1/memory/search", self.base_url);
        let body = SearchRequest {
            user_id,
            query,
            method: method.as_str(),
            top_k,
            app_id: "aigc-studio",
            project_id: "default",
        };
        let resp: SuccessEnvelope<SearchResult> = self
            .agent
            .post(&url)
            .send_json(&body)
            .map_err(|e| EverosError::Network(format!("search: {e}")))?
            .into_json()
            .map_err(|e| EverosError::Api(format!("search response parse: {e}")))?;
        Ok(resp.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_method_as_str() {
        assert_eq!(SearchMethod::Keyword.as_str(), "keyword");
        assert_eq!(SearchMethod::Hybrid.as_str(), "hybrid");
    }

    #[test]
    fn everos_message_serializes() {
        let msg = EverosMessage {
            sender_id: "student1".to_owned(),
            role: "user".to_owned(),
            timestamp: 1672531200000,
            content: "帮我画一只猫".to_owned(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("senderId"));
        assert!(json.contains("帮我画一只猫"));
    }

    #[test]
    fn search_result_deserializes() {
        let json = r#"{
            "requestId": "test",
            "data": {
                "episodes": [{
                    "id": "ep1",
                    "summary": "学生喜欢猫",
                    "score": 0.8,
                    "atomicFacts": [{"content": "学生喜欢猫", "score": 0.8}]
                }],
                "profiles": [],
                "agentCases": [],
                "atomicFacts": []
            }
        }"#;
        let resp: SuccessEnvelope<SearchResult> = serde_json::from_str(json).unwrap();
        assert_eq!(resp.data.episodes.len(), 1);
        assert_eq!(resp.data.episodes[0].atomic_facts[0].content, "学生喜欢猫");
    }
}
