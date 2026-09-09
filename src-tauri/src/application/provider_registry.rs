//! Provider 注册表。
//!
//! 管理所有已注册的 Provider 适配器，支持按 ID 查找和按能力类型查找。
//! PollWorker 和 Pipeline 通过此注册表获取 Provider 实例。
//!
//! Phase A 重构：注册表同时存储 credential_key（凭据记录 ID），
//! 并通过 CredentialManager 在调用时解析 CredentialContext。
//! Provider 适配器无状态（不持有密钥），每次调用通过 CredentialContext 获取认证信息。

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use crate::application::credential_manager::CredentialManager;
use crate::domain::credentials::CredentialContext;
use crate::ports::unified_provider::{CapabilityKind, UnifiedProviderAdapter};

/// 已解析的 Provider：适配器 + 凭据上下文。
///
/// 调用方通过 `ProviderRegistry::resolve()` 获取此结构，
/// 然后使用 `adapter.submit(&request, &credential)` 等方法。
pub struct ResolvedProvider {
    pub adapter: Arc<dyn UnifiedProviderAdapter>,
    pub credential: CredentialContext,
}

/// Provider 注册表。
pub struct ProviderRegistry {
    /// provider_id → (adapter, credential_key)
    #[allow(clippy::type_complexity)]
    adapters: RwLock<HashMap<String, (Arc<dyn UnifiedProviderAdapter>, Option<String>)>>,
    /// 凭据解析器（Phase A 注入，setup 阶段设置一次）。
    credential_manager: OnceLock<Arc<CredentialManager>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            adapters: RwLock::new(HashMap::new()),
            credential_manager: OnceLock::new(),
        }
    }

    /// 注入 CredentialManager（在应用初始化时调用一次）。
    /// 重复调用会被忽略（OnceLock 语义）。
    pub fn set_credential_manager(&self, manager: Arc<CredentialManager>) {
        let _ = self.credential_manager.set(manager);
    }

    /// 注册 Provider（带 credential_key）。
    pub fn register_with_credential(
        &self,
        adapter: Arc<dyn UnifiedProviderAdapter>,
        credential_key: String,
    ) {
        let id = adapter.provider_id().to_owned();
        self.adapters
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, (adapter, Some(credential_key)));
    }

    /// 注册 Provider（无 credential，用于测试或本地 Provider）。
    pub fn register(&self, adapter: Arc<dyn UnifiedProviderAdapter>) {
        let id = adapter.provider_id().to_owned();
        self.adapters
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, (adapter, None));
    }

    pub fn clear(&self) {
        self.adapters
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    /// 解析 Provider + CredentialContext（主入口）。
    ///
    /// 从注册表获取适配器和 credential_key，然后通过 CredentialManager 解析出
    /// Provider 调用所需的 CredentialContext。
    pub fn resolve(&self, provider_id: &str) -> Option<ResolvedProvider> {
        let (adapter, credential_key) = {
            let adapters = self.adapters.read().unwrap_or_else(|e| e.into_inner());
            adapters.get(provider_id).cloned()
        }?;

        let credential = self.resolve_credential(credential_key.as_deref(), adapter.provider_id());
        Some(ResolvedProvider {
            adapter,
            credential,
        })
    }

    /// 按别名解析 Provider + CredentialContext。
    pub fn resolve_by_alias(&self, provider_name: &str) -> Option<ResolvedProvider> {
        let (adapter, credential_key) = self.find_by_alias(provider_name)?;
        let credential = self.resolve_credential(credential_key.as_deref(), adapter.provider_id());
        Some(ResolvedProvider {
            adapter,
            credential,
        })
    }

    /// 按 ID 获取适配器（不含 credential，向后兼容/测试用）。
    pub fn get(&self, provider_id: &str) -> Option<Arc<dyn UnifiedProviderAdapter>> {
        self.adapters
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(provider_id)
            .map(|(adapter, _)| adapter.clone())
    }

    /// 按 ID 或常见别名获取适配器（不含 credential）。
    pub fn get_by_alias(&self, provider_name: &str) -> Option<Arc<dyn UnifiedProviderAdapter>> {
        self.find_by_alias(provider_name)
            .map(|(adapter, _)| adapter)
    }

    /// 按能力类型获取第一个可用 Provider。
    pub fn resolve_capability(&self, capability: &CapabilityKind) -> Option<ResolvedProvider> {
        let (adapter, credential_key) = {
            let adapters = self.adapters.read().unwrap_or_else(|e| e.into_inner());
            adapters
                .values()
                .find(|(a, _)| a.capabilities().contains(capability))
                .cloned()
        }?;
        let credential = self.resolve_credential(credential_key.as_deref(), adapter.provider_id());
        Some(ResolvedProvider {
            adapter,
            credential,
        })
    }

    /// 按能力类型获取第一个可用 Provider（不含 credential，向后兼容）。
    pub fn resolve_adapter(
        &self,
        capability: &CapabilityKind,
    ) -> Option<Arc<dyn UnifiedProviderAdapter>> {
        self.adapters
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .find(|(a, _)| a.capabilities().contains(capability))
            .map(|(a, _)| a.clone())
    }

    /// 列出所有已注册 Provider ID。
    pub fn list_ids(&self) -> Vec<String> {
        self.adapters
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect()
    }

    /// 列出支持指定能力的所有 Provider。
    pub fn list_by_capability(
        &self,
        capability: &CapabilityKind,
    ) -> Vec<Arc<dyn UnifiedProviderAdapter>> {
        self.adapters
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|(a, _)| a.capabilities().contains(capability))
            .map(|(a, _)| a.clone())
            .collect()
    }

    /// 已注册 Provider 数量。
    pub fn count(&self) -> usize {
        self.adapters
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    // ─── 内部方法 ───

    /// 内部别名查找。
    fn find_by_alias(
        &self,
        provider_name: &str,
    ) -> Option<(Arc<dyn UnifiedProviderAdapter>, Option<String>)> {
        let adapters = self.adapters.read().unwrap_or_else(|e| e.into_inner());

        // 精确匹配
        if let Some(entry) = adapters.get(provider_name) {
            return Some(entry.clone());
        }

        // 别名匹配
        let normalized = normalize_provider(provider_name);
        for alias in provider_aliases(&normalized) {
            if let Some(entry) = adapters.get(alias) {
                return Some(entry.clone());
            }
        }

        // 模糊匹配
        adapters
            .iter()
            .find(|(id, _)| {
                let normalized_id = normalize_provider(id);
                normalized.contains(&normalized_id) || normalized_id.contains(&normalized)
            })
            .map(|(_, entry)| entry.clone())
    }

    /// 解析 credential。如果 CredentialManager 未注入或 credential_key 为空，
    /// 返回一个占位 CredentialContext（允许无认证调用，如本地 Provider 或测试）。
    fn resolve_credential(
        &self,
        credential_key: Option<&str>,
        provider_id: &str,
    ) -> CredentialContext {
        let manager = self.credential_manager.get();
        eprintln!("[Registry] resolve_credential: key={credential_key:?}, provider={provider_id}, manager_set={}", manager.is_some());
        match (credential_key, manager) {
            (Some(key), Some(mgr)) => match mgr.resolve(key) {
                Ok(ctx) => ctx,
                Err(e) => {
                    eprintln!("[Registry] Failed to resolve credential for '{provider_id}': {e}");
                    placeholder_credential(provider_id)
                }
            },
            _ => placeholder_credential(provider_id),
        }
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 占位 CredentialContext（无认证信息）。
fn placeholder_credential(provider_id: &str) -> CredentialContext {
    CredentialContext {
        provider_id: provider_id.to_owned(),
        credential_type: crate::domain::credentials::CredentialType::ApiKey,
        payload: serde_json::json!({}),
        base_url: String::new(),
        model: String::new(),
    }
}

fn normalize_provider(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .replace([' ', '_', '-', '/', '·'], "")
}

fn provider_aliases(normalized: &str) -> Vec<&'static str> {
    let mut aliases = Vec::new();
    if normalized.contains("grok") || normalized.contains("xai") {
        aliases.push("grok");
    }
    if normalized.contains("kling") || normalized.contains("可灵") {
        aliases.push("kling");
    }
    if normalized.contains("seedance")
        || normalized.contains("doubao")
        || normalized.contains("volcengine")
        || normalized.contains("豆包")
        || normalized.contains("火山")
    {
        aliases.push("seedance");
    }
    aliases
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::providers::ProviderError;

    /// Mock Provider 用于测试。
    struct MockProvider {
        id: String,
        caps: Vec<CapabilityKind>,
        is_sync: bool,
    }

    impl UnifiedProviderAdapter for MockProvider {
        fn provider_id(&self) -> &str {
            &self.id
        }
        fn capabilities(&self) -> Vec<CapabilityKind> {
            self.caps.clone()
        }
        fn submit(
            &self,
            _request: &crate::ports::unified_provider::UnifiedRequest,
            _credential: &CredentialContext,
        ) -> Result<crate::ports::unified_provider::UnifiedSubmitResult, ProviderError> {
            Ok(crate::ports::unified_provider::UnifiedSubmitResult {
                remote_job_id: "mock-123".into(),
                initial_status: "succeeded".into(),
                immediate_result_url: Some("https://example.com/img.png".into()),
                estimated_duration_secs: Some(0),
            })
        }
        fn poll(
            &self,
            _id: &str,
            _credential: &CredentialContext,
        ) -> Result<crate::ports::unified_provider::UnifiedPollResult, ProviderError> {
            Ok(crate::ports::unified_provider::UnifiedPollResult {
                status: "succeeded".into(),
                progress: 100,
                result_url: None,
                error_message: None,
                retryable: false,
            })
        }
        fn download(
            &self,
            _url: &str,
            _dir: &std::path::Path,
            _credential: &CredentialContext,
        ) -> Result<crate::ports::unified_provider::UnifiedDownloadResult, ProviderError> {
            Ok(crate::ports::unified_provider::UnifiedDownloadResult {
                file_path: "/tmp/test.png".into(),
                mime_type: "image/png".into(),
                file_size: 1024,
                duration_secs: None,
                width: Some(512),
                height: Some(512),
            })
        }
        fn health_check(
            &self,
            _credential: &CredentialContext,
        ) -> Result<crate::ports::unified_provider::UnifiedHealthStatus, ProviderError> {
            Ok(crate::ports::unified_provider::UnifiedHealthStatus {
                available: true,
                message: "ok".into(),
                quota_remaining: Some(100),
            })
        }
        fn is_async(&self) -> bool {
            !self.is_sync
        }
    }

    #[test]
    fn register_and_get_by_id() {
        let registry = ProviderRegistry::new();
        registry.register(Arc::new(MockProvider {
            id: "grok".into(),
            caps: vec![CapabilityKind::TextToImage],
            is_sync: true,
        }));
        assert!(registry.get("grok").is_some());
        assert!(registry.get("kling").is_none());
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn resolve_returns_placeholder_without_manager() {
        let registry = ProviderRegistry::new();
        registry.register(Arc::new(MockProvider {
            id: "grok".into(),
            caps: vec![CapabilityKind::TextToImage],
            is_sync: true,
        }));
        let resolved = registry.resolve("grok").unwrap();
        assert_eq!(resolved.adapter.provider_id(), "grok");
        // 无 CredentialManager 时返回 placeholder
        assert_eq!(resolved.credential.provider_id, "grok");
    }

    #[test]
    fn resolve_by_capability() {
        let registry = ProviderRegistry::new();
        registry.register(Arc::new(MockProvider {
            id: "grok".into(),
            caps: vec![CapabilityKind::TextToImage],
            is_sync: true,
        }));
        registry.register(Arc::new(MockProvider {
            id: "kling".into(),
            caps: vec![CapabilityKind::TextToVideo, CapabilityKind::ImageToVideo],
            is_sync: false,
        }));

        assert!(registry
            .resolve_capability(&CapabilityKind::TextToImage)
            .is_some());
        assert!(registry
            .resolve_capability(&CapabilityKind::TextToVideo)
            .is_some());
        assert!(registry
            .resolve_capability(&CapabilityKind::TextToSpeech)
            .is_none());
    }

    #[test]
    fn list_by_capability() {
        let registry = ProviderRegistry::new();
        registry.register(Arc::new(MockProvider {
            id: "grok".into(),
            caps: vec![CapabilityKind::TextToImage],
            is_sync: true,
        }));
        registry.register(Arc::new(MockProvider {
            id: "sd".into(),
            caps: vec![CapabilityKind::TextToImage],
            is_sync: false,
        }));
        registry.register(Arc::new(MockProvider {
            id: "kling".into(),
            caps: vec![CapabilityKind::TextToVideo],
            is_sync: false,
        }));

        let image_providers = registry.list_by_capability(&CapabilityKind::TextToImage);
        assert_eq!(image_providers.len(), 2);

        let video_providers = registry.list_by_capability(&CapabilityKind::TextToVideo);
        assert_eq!(video_providers.len(), 1);
    }

    #[test]
    fn resolves_common_display_name_aliases() {
        let registry = ProviderRegistry::new();
        registry.register(Arc::new(MockProvider {
            id: "kling".into(),
            caps: vec![CapabilityKind::TextToVideo],
            is_sync: false,
        }));

        assert!(registry.get_by_alias("可灵视频账号").is_some());
        assert!(registry.get_by_alias("Kling").is_some());
        assert!(registry.get_by_alias("Seedance").is_none());
    }
}
