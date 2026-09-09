use std::io::{BufRead, BufReader};

use serde::Deserialize;

use crate::{
    domain::agent::{ChatRequest, ChatResponse, ToolCall},
    ports::agent_llm::{AgentLlm, AgentLlmError},
};

/// Grok Chat Completions 适配器：调用 xAI / 任意 OpenAI 兼容的 /v1/chat/completions 端点。
///
/// 与现有 OpenAiCompatibleAdapter 的关键区别：
/// - 后者调用 `/v1/generations`（异步生成任务 submit→poll→download）；
/// - 本适配器调用 `/v1/chat/completions`（同步阻塞 chat + tools），用于 Agent 工具循环。
///
/// 复用现有 ureq 同步 HTTP 客户端与 keychain 凭据，避免引入新依赖。
pub struct GrokChatAdapter {
    base_url: String,
    api_key: String,
    /// 可选请求超时（秒）。None 时使用 ureq 默认。
    timeout_secs: Option<u64>,
}

impl GrokChatAdapter {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            base_url,
            api_key,
            timeout_secs: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout_secs = Some(seconds);
        self
    }

    fn endpoint(&self) -> String {
        let trimmed = self.base_url.trim_end_matches('/');
        // 避免 base_url 已含 /v1 时产生 /v1/v1/chat/completions 的双重路径。
        if trimmed.ends_with("/v1") {
            format!("{}/chat/completions", trimmed)
        } else {
            format!("{}/v1/chat/completions", trimmed)
        }
    }

    fn build_agent(&self) -> Result<ureq::Agent, AgentLlmError> {
        // LLM 推理可能较慢，默认 120s 超时防止无限挂起。
        let timeout = self.timeout_secs.unwrap_or(120);
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(timeout))
            .build();
        Ok(agent)
    }
}

impl AgentLlm for GrokChatAdapter {
    fn chat(&mut self, request: &ChatRequest) -> Result<ChatResponse, AgentLlmError> {
        if self.api_key.trim().is_empty() {
            return Err(AgentLlmError::Config("missing api key".to_owned()));
        }
        if request.model.trim().is_empty() {
            return Err(AgentLlmError::Config("missing model name".to_owned()));
        }

        let endpoint = self.endpoint();
        eprintln!(
            "[GrokChat] POST {} model={} tools={}",
            endpoint,
            request.model,
            request.tools.len()
        );

        let body = serialize_request(request);
        let agent = self.build_agent()?;
        let response = send_json_with_retry(&agent, &endpoint, &self.api_key, body, "chat")?;

        let status = response.status();
        if status >= 400 {
            let body = response.into_string().unwrap_or_default();
            eprintln!("[GrokChat] remote error {status}: {body}");
            return Err(AgentLlmError::Remote { status, body });
        }

        let parsed: ChatCompletionResponse = response
            .into_json()
            .map_err(|e| AgentLlmError::MalformedResponse(e.to_string()))?;

        eprintln!(
            "[GrokChat] OK model={} finish={:?}",
            parsed.model,
            parsed
                .choices
                .first()
                .and_then(|c| c.finish_reason.as_deref())
        );
        parse_chat_response(parsed)
    }

    fn chat_stream(
        &mut self,
        request: &ChatRequest,
        on_chunk: &dyn Fn(&str),
    ) -> Result<ChatResponse, AgentLlmError> {
        if self.api_key.trim().is_empty() {
            return Err(AgentLlmError::Config("missing api key".to_owned()));
        }
        if request.model.trim().is_empty() {
            return Err(AgentLlmError::Config("missing model name".to_owned()));
        }

        let endpoint = self.endpoint();
        eprintln!(
            "[GrokChat] POST (stream) {} model={} tools={}",
            endpoint,
            request.model,
            request.tools.len()
        );

        // 强制开启流式。
        let mut stream_request = request.clone();
        stream_request.stream = true;
        let body = serialize_request(&stream_request);

        let agent = self.build_agent()?;
        let response = send_json_with_retry(&agent, &endpoint, &self.api_key, body, "stream")?;

        let status = response.status();
        if status >= 400 {
            let body = response.into_string().unwrap_or_default();
            eprintln!("[GrokChat] stream remote error {status}: {body}");
            return Err(AgentLlmError::Remote { status, body });
        }

        let mut reader = BufReader::new(response.into_reader());
        let mut accumulator = StreamAccumulator::new(request.model.clone());
        let mut line = String::new();
        loop {
            line.clear();
            let read = reader
                .read_line(&mut line)
                .map_err(|e| AgentLlmError::Network(format!("stream read error: {e}")))?;
            if read == 0 {
                break; // EOF
            }
            let trimmed = line.trim();
            let Some(payload) = trimmed.strip_prefix("data:") else {
                continue;
            };
            let payload = payload.trim();
            if payload == "[DONE]" {
                break;
            }
            let Ok(chunk) = serde_json::from_str::<StreamChunk>(payload) else {
                continue;
            };
            accumulator.apply(chunk, on_chunk);
        }

        eprintln!(
            "[GrokChat] stream done model={} finish={} tool_calls={} content_len={}",
            accumulator.model,
            accumulator.finish_reason.as_deref().unwrap_or("none"),
            accumulator.tool_calls.len(),
            accumulator.content.len()
        );

        // 兜底：流式解析未产生任何有效内容（content 为空且无工具调用），
        // 说明 SSE 解析可能静默失败，回退到非流式 chat() 确保用户得到响应。
        if accumulator.content.is_empty() && accumulator.tool_calls.is_empty() {
            eprintln!("[GrokChat] stream produced empty result, falling back to non-stream chat");
            return self.chat(request);
        }

        Ok(accumulator.into_response())
    }
}

/// 流式响应累积器：把 SSE 增量片段拼装为完整 ChatResponse。
#[derive(Default)]
struct StreamAccumulator {
    model: String,
    content: String,
    finish_reason: Option<String>,
    tool_calls: Vec<ToolCallAcc>,
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
}

#[derive(Default)]
struct ToolCallAcc {
    id: String,
    name: String,
    arguments: String,
}

impl StreamAccumulator {
    fn new(model: String) -> Self {
        Self {
            model,
            ..Default::default()
        }
    }

    fn apply(&mut self, chunk: StreamChunk, on_chunk: &dyn Fn(&str)) {
        if !chunk.model.is_empty() {
            self.model = chunk.model;
        }
        if let Some(usage) = chunk.usage {
            self.prompt_tokens = Some(usage.prompt_tokens);
            self.completion_tokens = Some(usage.completion_tokens);
        }
        let Some(choice) = chunk.choices.into_iter().next() else {
            return;
        };
        if let Some(reason) = choice.finish_reason {
            if !reason.is_empty() {
                self.finish_reason = Some(reason);
            }
        }
        let delta = choice.delta;
        // 文本内容增量：回调给调用方做实时展示。
        if let Some(piece) = delta.content {
            if !piece.is_empty() {
                self.content.push_str(&piece);
                on_chunk(&piece);
            }
        }
        // 工具调用增量：按 index 累积 id / name / arguments。
        for tc in delta.tool_calls {
            let idx = tc.index as usize;
            if self.tool_calls.len() <= idx {
                self.tool_calls.resize_with(idx + 1, ToolCallAcc::default);
            }
            let acc = &mut self.tool_calls[idx];
            if let Some(id) = tc.id {
                acc.id = id;
            }
            if let Some(function) = tc.function {
                if let Some(name) = function.name {
                    acc.name = name;
                }
                if let Some(args) = function.arguments {
                    acc.arguments.push_str(&args);
                }
            }
        }
    }

    fn into_response(self) -> ChatResponse {
        let tool_calls: Vec<ToolCall> = self
            .tool_calls
            .into_iter()
            .filter(|tc| !tc.id.is_empty() || !tc.name.is_empty())
            .map(|tc| {
                let id = if tc.id.is_empty() {
                    uuid::Uuid::new_v4().to_string()
                } else {
                    tc.id
                };
                ToolCall::new(id, tc.name, tc.arguments)
            })
            .collect();
        // 与非流式语义保持一致：有工具调用且无文本时 content 为 None。
        let content = if self.content.is_empty() && !tool_calls.is_empty() {
            None
        } else {
            Some(self.content)
        };
        ChatResponse {
            content,
            tool_calls,
            finish_reason: self.finish_reason.unwrap_or_else(|| "stop".to_owned()),
            model: self.model,
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
        }
    }
}

/// 单次请求最大尝试次数（1 次原始请求 + 2 次瞬时错误重试）。
const CHAT_MAX_ATTEMPTS: u32 = 3;

/// 发送 POST 并对瞬时传输错误自动重试（退避 1s、2s）。
///
/// 仅在读到响应头之前失败时重试：连接建立失败、DNS 解析失败、
/// 读状态行时连接中断（如 os error 10060）多为瞬时网络抖动；
/// HTTP 4xx/5xx 是服务端的确定性回答，直接返回不重试。
/// 已进入流式读取阶段后的错误不在此重试，避免向 UI 重复推送内容。
fn send_json_with_retry(
    agent: &ureq::Agent,
    endpoint: &str,
    api_key: &str,
    body: serde_json::Value,
    label: &str,
) -> Result<ureq::Response, AgentLlmError> {
    for attempt in 1..=CHAT_MAX_ATTEMPTS {
        let result = agent
            .post(endpoint)
            .set("Authorization", &format!("Bearer {api_key}"))
            .set("Content-Type", "application/json")
            .send_json(body.clone());
        match result {
            Ok(response) => return Ok(response),
            Err(ureq::Error::Status(status, response)) => {
                let body = response.into_string().unwrap_or_default();
                eprintln!("[GrokChat] {label} remote error {status}: {body}");
                return Err(AgentLlmError::Remote { status, body });
            }
            Err(e) if attempt < CHAT_MAX_ATTEMPTS && is_transient_transport_error(&e) => {
                eprintln!(
                    "[GrokChat] {label} transient transport error (attempt {attempt}/{CHAT_MAX_ATTEMPTS}), retrying: {e}"
                );
                std::thread::sleep(std::time::Duration::from_secs(u64::from(attempt)));
            }
            Err(e) => {
                eprintln!("[GrokChat] {label} transport error: {e}");
                return Err(classify_transport_error(e));
            }
        }
    }
    unreachable!("retry loop returns on every terminal branch")
}

fn is_transient_transport_error(error: &ureq::Error) -> bool {
    match error {
        ureq::Error::Transport(transport) => matches!(
            transport.kind(),
            ureq::ErrorKind::Io | ureq::ErrorKind::ConnectionFailed | ureq::ErrorKind::Dns
        ),
        ureq::Error::Status(_, _) => false,
    }
}

fn classify_transport_error(error: ureq::Error) -> AgentLlmError {
    match error {
        ureq::Error::Status(status, response) => {
            let body = response.into_string().unwrap_or_default();
            eprintln!(
                "[GrokChat] remote {status} body: {}",
                body.chars().take(500).collect::<String>()
            );
            AgentLlmError::Remote { status, body }
        }
        other => AgentLlmError::Network(other.to_string()),
    }
}

/// 构造 OpenAI Chat Completions 请求体。
fn serialize_request(request: &ChatRequest) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": request.model,
        "messages": request.messages,
    });
    if !request.tools.is_empty() {
        body["tools"] = serde_json::to_value(&request.tools).unwrap_or(serde_json::Value::Null);
        // tool_choice 默认 "auto"：让 LLM 自行决定是否调用工具。
        body["tool_choice"] = serde_json::Value::String("auto".to_owned());
    }
    if request.stream {
        body["stream"] = serde_json::Value::Bool(true);
    }
    body
}

/// 解析 OpenAI Chat Completions 响应（取首个 choice）。
fn parse_chat_response(parsed: ChatCompletionResponse) -> Result<ChatResponse, AgentLlmError> {
    let choice = parsed
        .choices
        .first()
        .ok_or_else(|| AgentLlmError::MalformedResponse("missing choices".to_owned()))?;
    let message = &choice.message;
    let mut tool_calls = message.tool_calls.clone().unwrap_or_default();
    // LLM 可能返回空 id 的 tool_call，兜底生成 UUID。
    for tc in &mut tool_calls {
        if tc.id.trim().is_empty() {
            tc.id = uuid::Uuid::new_v4().to_string();
        }
    }
    if tool_calls.is_empty() && message.content.is_none() {
        return Err(AgentLlmError::MalformedResponse(
            "assistant message has neither content nor tool_calls".to_owned(),
        ));
    }
    Ok(ChatResponse {
        content: message.content.clone(),
        tool_calls,
        finish_reason: choice
            .finish_reason
            .clone()
            .unwrap_or_else(|| "stop".to_owned()),
        model: parsed.model.clone(),
        prompt_tokens: parsed.usage.as_ref().map(|u| u.prompt_tokens),
        completion_tokens: parsed.usage.as_ref().map(|u| u.completion_tokens),
    })
}

// ──────────────────────────────────────────────────────────────────
// OpenAI Chat Completions wire types
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    #[serde(default)]
    choices: Vec<Choice>,
    model: String,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: i64,
    completion_tokens: i64,
}

// ──────────────────────────────────────────────────────────────────
// OpenAI streaming (text/event-stream) wire types
// 每个 chunk 携带增量 delta，而非完整 message。
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    #[serde(default)]
    model: String,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<StreamToolCallDelta>,
}

#[derive(Debug, Deserialize)]
struct StreamToolCallDelta {
    #[serde(default)]
    index: u32,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<StreamFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct StreamFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

// 工具定义序列化测试用例。
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::agent::{ChatMessage, ToolDefinition};

    #[test]
    fn serializes_basic_request() {
        let request = ChatRequest::new(
            "grok-4".to_owned(),
            vec![
                ChatMessage::system("你是助手".to_owned()),
                ChatMessage::user("画一张猫".to_owned()),
            ],
            vec![],
        );
        let body = serialize_request(&request);
        assert_eq!(body["model"], "grok-4");
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn serializes_request_with_tools() {
        let parameters = serde_json::json!({
            "type": "object",
            "properties": {
                "prompt": {"type": "string"}
            },
            "required": ["prompt"]
        });
        let tool = ToolDefinition::function("image_generation", "根据提示词生成图片", parameters);
        let request = ChatRequest::new(
            "grok-4".to_owned(),
            vec![ChatMessage::user("画猫".to_owned())],
            vec![tool],
        );
        let body = serialize_request(&request);
        assert!(body.get("tools").is_some());
        assert_eq!(body["tool_choice"], "auto");
    }

    #[test]
    fn serializes_tool_history_with_snake_case_keys() {
        // OpenAI 协议要求 assistant 的 tool_calls 与 tool 消息的 tool_call_id
        // 必须是蛇形命名；camelCase 会导致 DeepSeek 等供应商返回 400。
        let request = ChatRequest::new(
            "deepseek-v4-pro".to_owned(),
            vec![
                ChatMessage::user("画猫".to_owned()),
                ChatMessage::assistant(
                    None,
                    vec![crate::domain::agent::ToolCall::new(
                        "call_1".to_owned(),
                        "image_generation".to_owned(),
                        "{\"prompt\":\"猫\"}".to_owned(),
                    )],
                ),
                ChatMessage::tool("call_1".to_owned(), "{\"status\":\"ok\"}".to_owned()),
            ],
            vec![],
        );
        let body = serialize_request(&request);
        let messages = body["messages"].as_array().unwrap();
        let assistant = &messages[1];
        assert!(
            assistant.get("tool_calls").is_some(),
            "assistant 消息必须使用蛇形 tool_calls"
        );
        assert!(assistant.get("toolCalls").is_none());
        let tool = &messages[2];
        assert_eq!(tool["tool_call_id"], "call_1");
        assert!(tool.get("toolCallId").is_none());
    }

    #[test]
    fn parses_stop_response() {
        let raw = ChatCompletionResponse {
            choices: vec![Choice {
                message: ResponseMessage {
                    content: Some("好的，我帮你画".to_owned()),
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_owned()),
            }],
            model: "grok-4".to_owned(),
            usage: Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
            }),
        };
        let parsed = parse_chat_response(raw).unwrap();
        assert_eq!(parsed.finish_reason, "stop");
        assert!(parsed.tool_calls.is_empty());
        assert_eq!(parsed.prompt_tokens, Some(10));
    }

    #[test]
    fn parses_tool_calls_response() {
        let raw = ChatCompletionResponse {
            choices: vec![Choice {
                message: ResponseMessage {
                    content: None,
                    tool_calls: Some(vec![ToolCall::new(
                        "call_1".to_owned(),
                        "image_generation".to_owned(),
                        r#"{"prompt":"猫"}"#.to_owned(),
                    )]),
                },
                finish_reason: Some("tool_calls".to_owned()),
            }],
            model: "grok-4".to_owned(),
            usage: None,
        };
        let parsed = parse_chat_response(raw).unwrap();
        assert_eq!(parsed.finish_reason, "tool_calls");
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].function.name, "image_generation");
    }

    #[test]
    fn rejects_empty_choices() {
        let raw = ChatCompletionResponse {
            choices: vec![],
            model: "grok-4".to_owned(),
            usage: None,
        };
        let error = parse_chat_response(raw).unwrap_err();
        assert!(matches!(error, AgentLlmError::MalformedResponse(_)));
    }
}
