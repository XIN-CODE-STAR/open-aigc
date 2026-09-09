use thiserror::Error;

use crate::domain::agent::{ChatRequest, ChatResponse};

/// LLM 调用失败的错误类型。
#[derive(Debug, Error)]
pub enum AgentLlmError {
    /// 网络层错误（DNS、连接、超时等）。
    #[error("chat LLM network error: {0}")]
    Network(String),
    /// 远端返回非 2xx 状态码，附带响应体用于诊断。
    #[error("chat LLM remote error ({status}): {body}")]
    Remote { status: u16, body: String },
    /// 响应体无法解析为 Chat Completions 协议结构。
    #[error("chat LLM malformed response: {0}")]
    MalformedResponse(String),
    /// 配置错误（缺 API key、不支持的模型等）。
    #[error("chat LLM config error: {0}")]
    Config(String),
}

/// Chat LLM 端口：抽象 OpenAI 兼容的 Chat Completions 调用。
///
/// 实现需保证：
/// - `chat` 同步阻塞调用，调用方负责放入 spawn_blocking。
/// - `chat_stream` 流式调用，通过回调逐块返回内容。
/// - 请求体严格遵循 OpenAI Chat Completions 协议。
/// - 错误必须按 Network/Remote/MalformedResponse/Config 分类。
pub trait AgentLlm: Send {
    fn chat(&mut self, request: &ChatRequest) -> Result<ChatResponse, AgentLlmError>;

    /// 流式调用：逐块返回内容片段，最终返回完整响应。
    /// 默认实现回退到非流式 chat。
    fn chat_stream(
        &mut self,
        request: &ChatRequest,
        on_chunk: &dyn Fn(&str),
    ) -> Result<ChatResponse, AgentLlmError> {
        // 默认回退到非流式
        let response = self.chat(request)?;
        if let Some(content) = &response.content {
            on_chunk(content);
        }
        Ok(response)
    }
}
