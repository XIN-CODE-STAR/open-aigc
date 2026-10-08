//! 记忆服务实现。
#![allow(dead_code)]
//!
//! 通过 EverOS HTTP 客户端提供长期记忆能力。
//! 对话结束后自动存储，规划阶段自动检索。

use std::sync::{Arc, Mutex};

use crate::adapters::everos::{EverosClient, EverosError, EverosProcessManager};
use crate::application::error::AppError;
use crate::domain::agent::{MessageRecord, MessageRole};
use crate::ports::memory_service::{MemoryResult, MemoryServicePort};

/// 记忆服务配置。
#[derive(Debug, Clone)]
pub struct MemoryServiceConfig {
    pub enabled: bool,
    pub port: u16,
    pub root_path: String,
    pub search_method: String,
}

impl Default for MemoryServiceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 18000,
            root_path: String::new(),
            search_method: "keyword".to_owned(),
        }
    }
}

/// 记忆服务实现。通过 Arc 共享，支持 Clone。
#[derive(Clone)]
pub struct MemoryServiceImpl {
    inner: Arc<MemoryServiceInner>,
}

struct MemoryServiceInner {
    client: EverosClient,
    process_manager: EverosProcessManager,
    /// 运行期可变：设置界面可启用/停用长期记忆并切换根目录。
    config: Mutex<MemoryServiceConfig>,
}

/// EverOS 配置与运行状态（供设置界面展示）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EverosStatus {
    /// 是否已启用（启用后记忆才参与对话）。
    pub enabled: bool,
    /// 子进程是否在运行。
    pub running: bool,
    /// 健康检查是否通过（服务可用）。
    pub available: bool,
    /// 服务端口。
    pub port: u16,
    /// 记忆根目录。
    pub root_path: String,
    /// 根目录下是否存在 everos.toml（缺失需先运行 scripts/setup-everos.ps1）。
    pub config_present: bool,
}

impl MemoryServiceImpl {
    /// 创建新的记忆服务。
    pub fn new(config: MemoryServiceConfig) -> Self {
        let client = EverosClient::new(config.port);
        let process_manager =
            EverosProcessManager::new(config.port, std::path::PathBuf::from(&config.root_path));
        Self {
            inner: Arc::new(MemoryServiceInner {
                client,
                process_manager,
                config: Mutex::new(config),
            }),
        }
    }

    /// 读取配置快照（锁内克隆，避免持锁跨调用）。
    fn config_snapshot(&self) -> MemoryServiceConfig {
        self.inner
            .config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// 当前 EverOS 配置与运行状态。
    pub fn status(&self) -> EverosStatus {
        let config = self.config_snapshot();
        let root = std::path::Path::new(&config.root_path);
        EverosStatus {
            enabled: config.enabled,
            running: self.inner.process_manager.is_process_alive(),
            available: config.enabled && self.inner.client.health().unwrap_or(false),
            port: config.port,
            root_path: config.root_path.clone(),
            config_present: root.join("everos.toml").exists(),
        }
    }

    /// 启用 / 停用长期记忆。
    ///
    /// 启用：创建记忆根目录（可覆盖路径）→ 置 enabled → 尽力拉起服务
    /// （缺 everos CLI 或 everos.toml 时记录错误但不回滚启用状态，便于用户先配置再重启）。
    /// 停用：停止服务并置 enabled = false，**保留数据目录**。
    pub fn set_enabled(
        &self,
        enabled: bool,
        root_path: Option<String>,
    ) -> Result<EverosStatus, AppError> {
        if enabled {
            let current = self.config_snapshot();
            let root = root_path
                .filter(|p| !p.trim().is_empty())
                .unwrap_or(current.root_path);
            let path = std::path::PathBuf::from(&root);
            std::fs::create_dir_all(&path)
                .map_err(|e| AppError::MemoryServiceError(format!("创建记忆目录失败: {e}")))?;
            {
                let mut config = self.inner.config.lock().unwrap_or_else(|e| e.into_inner());
                config.enabled = true;
                config.root_path = root;
            }
            if let Err(e) = self.inner.process_manager.start() {
                eprintln!("[Memory] EverOS 服务启动失败（配置仍已启用）: {e}");
            }
        } else {
            let _ = self.inner.process_manager.stop();
            let mut config = self.inner.config.lock().unwrap_or_else(|e| e.into_inner());
            config.enabled = false;
        }
        Ok(self.status())
    }
}

impl MemoryServicePort for MemoryServiceImpl {
    fn remember_conversation(
        &self,
        _workspace_id: &str,
        conversation_id: &str,
        messages: &[MessageRecord],
    ) -> Result<(), AppError> {
        if !self.config_snapshot().enabled {
            return Ok(());
        }

        // 将 MessageRecord 转换为 EverosMessage。
        let everos_messages: Vec<crate::adapters::everos::client::EverosMessage> = messages
            .iter()
            .filter(|m| m.role == MessageRole::User || m.role == MessageRole::Assistant)
            .filter_map(|m| {
                let content = m.content.clone()?;
                let timestamp = parse_timestamp(&m.created_at);
                Some(crate::adapters::everos::client::EverosMessage {
                    sender_id: format!("{}-{}", m.role.as_str(), &m.id[..8.min(m.id.len())]),
                    role: m.role.as_str().to_owned(),
                    timestamp,
                    content,
                })
            })
            .collect();

        if everos_messages.is_empty() {
            return Ok(());
        }

        // 发送到 EverOS。
        let session_id = format!("conv-{conversation_id}");
        let _ = self
            .inner
            .client
            .add_messages(&session_id, &everos_messages);
        let _ = self.inner.client.flush(&session_id);

        Ok(())
    }

    /// 事实沉淀：作为 assistant 消息写入会话记忆（EverOS 以消息为记忆载体）。
    fn remember_facts(&self, workspace_id: &str, facts: &[String]) -> Result<(), AppError> {
        if !self.config_snapshot().enabled || facts.is_empty() {
            return Ok(());
        }
        let timestamp = time::OffsetDateTime::now_utc().unix_timestamp();
        let messages: Vec<crate::adapters::everos::client::EverosMessage> = facts
            .iter()
            .map(|content| crate::adapters::everos::client::EverosMessage {
                sender_id: "canvas-fact".to_owned(),
                role: "assistant".to_owned(),
                timestamp,
                content: content.clone(),
            })
            .collect();
        let session_id = format!("conv-{workspace_id}");
        let _ = self.inner.client.add_messages(&session_id, &messages);
        let _ = self.inner.client.flush(&session_id);
        Ok(())
    }

    fn recall(
        &self,
        workspace_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryResult>, AppError> {
        if !self.config_snapshot().enabled {
            return Ok(Vec::new());
        }

        let primary = match self.config_snapshot().search_method.as_str() {
            "vector" => crate::adapters::everos::client::SearchMethod::Vector,
            "hybrid" => crate::adapters::everos::client::SearchMethod::Hybrid,
            _ => crate::adapters::everos::client::SearchMethod::Keyword,
        };

        // hybrid/vector 失败（如嵌入模型不可用）时降级为 keyword，保证记忆可用性。
        let result = match self
            .inner
            .client
            .search(query, workspace_id, primary, limit as i32)
        {
            Ok(r) => r,
            Err(e)
                if !matches!(
                    primary,
                    crate::adapters::everos::client::SearchMethod::Keyword
                ) =>
            {
                eprintln!(
                    "[Memory] {} search failed ({e}), falling back to keyword",
                    primary.as_str()
                );
                self.inner
                    .client
                    .search(
                        query,
                        workspace_id,
                        crate::adapters::everos::client::SearchMethod::Keyword,
                        limit as i32,
                    )
                    .map_err(map_everos_error)?
            }
            Err(e) => return Err(map_everos_error(e)),
        };

        let mut memories = Vec::new();

        // 提取 episodes。
        for ep in &result.episodes {
            let content = ep
                .summary
                .clone()
                .or_else(|| ep.episode.clone())
                .unwrap_or_default();
            if !content.is_empty() {
                memories.push(MemoryResult {
                    source_type: "episode".to_owned(),
                    content,
                    score: ep.score,
                    timestamp: None,
                });
            }
            // 提取 atomic facts。
            for fact in &ep.atomic_facts {
                memories.push(MemoryResult {
                    source_type: "fact".to_owned(),
                    content: fact.content.clone(),
                    score: fact.score,
                    timestamp: None,
                });
            }
        }

        // 提取 profiles。
        for profile in &result.profiles {
            if let Some(data) = &profile.profile_data {
                let content = serde_json::to_string(data).unwrap_or_default();
                memories.push(MemoryResult {
                    source_type: "profile".to_owned(),
                    content,
                    score: profile.score.unwrap_or(0.0),
                    timestamp: None,
                });
            }
        }

        // 按评分排序，取 top N。
        memories.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        memories.truncate(limit);

        Ok(memories)
    }

    fn is_available(&self) -> bool {
        if !self.config_snapshot().enabled {
            return false;
        }
        self.inner.client.health().unwrap_or(false)
    }

    fn start(&self) -> Result<(), AppError> {
        if !self.config_snapshot().enabled {
            return Ok(());
        }
        self.inner
            .process_manager
            .start()
            .map_err(|e| AppError::MemoryServiceError(e.to_string()))
    }

    fn stop(&self) -> Result<(), AppError> {
        self.inner
            .process_manager
            .stop()
            .map_err(|e| AppError::MemoryServiceError(e.to_string()))
    }
}

/// 将 EverOS 错误映射为应用错误。
fn map_everos_error(e: EverosError) -> AppError {
    match e {
        EverosError::NotAvailable => AppError::MemoryServiceUnavailable,
        other => AppError::MemoryServiceError(other.to_string()),
    }
}

/// 将 ISO 8601 时间戳转换为 Unix epoch 毫秒。
fn parse_timestamp(iso: &str) -> i64 {
    // 尝试解析 RFC3339 格式。
    if let Ok(dt) = time::OffsetDateTime::parse(iso, &time::format_description::well_known::Rfc3339)
    {
        return dt.unix_timestamp() * 1000 + (dt.nanosecond() / 1_000_000) as i64;
    }
    // 回退：返回当前时间。
    time::OffsetDateTime::now_utc().unix_timestamp() * 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_disabled() {
        let config = MemoryServiceConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.port, 18000);
        assert_eq!(config.search_method, "keyword");
    }

    #[test]
    fn parse_timestamp_handles_rfc3339() {
        let ts = parse_timestamp("2025-01-01T00:00:00Z");
        assert!(ts > 0);
    }

    #[test]
    fn parse_timestamp_handles_invalid() {
        let ts = parse_timestamp("not a date");
        assert!(ts > 0); // falls back to now
    }
}
