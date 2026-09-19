//! 极简 MCP stdio 客户端。
//!
//! 按需拉起服务器进程（stdio 传输，换行分隔 JSON-RPC），完成一次
//! `initialize → tools/list | tools/call` 往返后关闭进程。无状态设计
//! 换取零生命周期管理成本：每次调用都是干净会话，服务器崩溃不影响宿主。
//!
//! 协议参考 MCP 规范 2024-11-05：
//! 1. → `initialize`（协议版本 + clientInfo）
//! 2. ← 结果；→ `notifications/initialized`（无响应）
//! 3. → `tools/list` / `tools/call`，读取同 id 响应，跳过通知。

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::application::error::AppError;
use crate::application::plugin_service::McpServerConfig;
use crate::ports::persistence::PersistenceError;

const PROTOCOL_VERSION: &str = "2024-11-05";
const CLIENT_NAME: &str = "aigc-studio";
const CLIENT_VERSION: &str = "0.2.0";
/// 单次 RPC 超时（服务器冷启动如 npx 下载可能较慢）。
const RPC_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
}

fn mcp_err(operation: &'static str, message: impl Into<String>) -> AppError {
    AppError::Persistence(PersistenceError::new(
        operation,
        std::io::Error::other(message.into()),
    ))
}

struct McpSession {
    child: Child,
    stdin: std::process::ChildStdin,
    responses: mpsc::Receiver<Value>,
}

impl Drop for McpSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl McpSession {
    fn start(config: &McpServerConfig) -> Result<Self, AppError> {
        let mut child = Command::new(&config.command)
            .args(&config.args)
            .envs(&config.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| mcp_err("spawn mcp server", format!("{}: {e}", config.command)))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| mcp_err("spawn mcp server", "stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| mcp_err("spawn mcp server", "stdout unavailable"))?;

        // 独立线程读 stdout，按行解析 JSON，通过 channel 交给主线程（带超时）
        let (tx, responses) = mpsc::channel::<Value>();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    continue; // 非法行 / 服务器日志输出：跳过
                };
                if tx.send(value).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            responses,
        })
    }

    fn send(&mut self, payload: &Value) -> Result<(), AppError> {
        let mut line = payload.to_string();
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .map_err(|e| mcp_err("mcp send", e.to_string()))?;
        self.stdin
            .flush()
            .map_err(|e| mcp_err("mcp send", e.to_string()))
    }

    /// 读取指定 id 的响应；跳过通知与其他 id。
    fn receive(&mut self, id: u64) -> Result<Value, AppError> {
        let deadline = std::time::Instant::now() + RPC_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(mcp_err("mcp receive", "等待 MCP 服务器响应超时"));
            }
            match self.responses.recv_timeout(remaining) {
                Ok(message) => {
                    let Some(message_id) = message.get("id").and_then(Value::as_u64) else {
                        continue; // 通知 / 日志消息
                    };
                    if message_id != id {
                        continue;
                    }
                    if let Some(error) = message.get("error") {
                        let text = error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown error");
                        return Err(mcp_err("mcp error", text));
                    }
                    return Ok(message);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(mcp_err("mcp receive", "等待 MCP 服务器响应超时"));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(mcp_err(
                        "mcp receive",
                        "MCP 服务器进程意外退出（检查命令与参数是否正确）",
                    ));
                }
            }
        }
    }

    fn initialize(&mut self) -> Result<(), AppError> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": CLIENT_NAME, "version": CLIENT_VERSION }
            }
        });
        self.send(&request)?;
        self.receive(1)?;
        // initialized 通知无响应
        self.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))?;
        Ok(())
    }
}

/// 列出 MCP 服务器的工具（拉起进程 → initialize → tools/list → 关闭）。
pub fn list_tools(config: &McpServerConfig) -> Result<Vec<McpToolInfo>, AppError> {
    let mut session = McpSession::start(config)?;
    session.initialize()?;
    session.send(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}))?;
    let response = session.receive(2)?;
    let mut tools = Vec::new();
    if let Some(items) = response
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(Value::as_array)
    {
        for item in items {
            let Some(name) = item.get("name").and_then(Value::as_str) else {
                continue;
            };
            tools.push(McpToolInfo {
                name: name.to_owned(),
                description: item
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            });
        }
    }
    Ok(tools)
}

/// 调用 MCP 工具，返回拼接后的文本内容（LLM 可直接消费）。
pub fn call_tool(
    config: &McpServerConfig,
    tool: &str,
    arguments: Value,
) -> Result<String, AppError> {
    let mut session = McpSession::start(config)?;
    session.initialize()?;
    session.send(&json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": tool, "arguments": arguments }
    }))?;
    let response = session.receive(2)?;
    let result = response.get("result").cloned().unwrap_or(Value::Null);
    if let Some(is_error) = result.get("isError").and_then(Value::as_bool) {
        if is_error {
            let text = extract_text(&result);
            return Err(mcp_err("mcp call", format!("工具返回错误：{text}")));
        }
    }
    Ok(extract_text(&result))
}

/// 从 result.content 数组提取文本（type=text 拼接；其余类型标注占位）。
fn extract_text(result: &Value) -> String {
    let Some(items) = result.get("content").and_then(Value::as_array) else {
        return serde_json::to_string_pretty(result).unwrap_or_else(|_| "{}".to_owned());
    };
    let mut parts = Vec::new();
    for item in items {
        match item.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    parts.push(text.to_owned());
                }
            }
            Some(other) => parts.push(format!("[{other} 内容已省略]")),
            None => {}
        }
    }
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_text_joins_text_content_and_marks_other_types() {
        let result = json!({
            "content": [
                { "type": "text", "text": "第一段" },
                { "type": "image", "data": "..." },
                { "type": "text", "text": "第二段" }
            ]
        });
        let text = extract_text(&result);
        assert_eq!(text, "第一段\n[image 内容已省略]\n第二段");
    }

    #[test]
    fn extract_text_serializes_missing_content() {
        let text = extract_text(&json!({ "structured": { "k": 1 } }));
        assert!(text.contains("\"k\""));
    }
}
