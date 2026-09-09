//! Connector → Adapter 桥接器。
//!
//! 将 ResourceConnector（账号类）包装为 UnifiedProviderAdapter（API 类），
//! 使其可以注册到现有 ProviderRegistry 中，无需修改 Registry 和 Generation Engine。
//!
//! 设计意图：渐进式迁移。当前阶段 connector 和 adapter 共存于同一个 Registry，
//! 后续 Phase 如果 Registry 需要拆分为 ResourceRegistry，只需修改注册逻辑，
//! 上层调用链（GenerationFacade / PollWorker）完全不变。

use std::path::Path;
use std::sync::Arc;

use crate::domain::credentials::CredentialContext;
use crate::domain::providers::ProviderError;
use crate::ports::resource_connector::ResourceConnector;
use crate::ports::unified_provider::{
    CapabilityKind, UnifiedDownloadResult, UnifiedHealthStatus, UnifiedPollResult,
    UnifiedProviderAdapter, UnifiedRequest, UnifiedSubmitResult,
};

/// 将 ResourceConnector 桥接为 UnifiedProviderAdapter。
pub struct ConnectorAdapterBridge {
    connector: Arc<dyn ResourceConnector>,
}

impl ConnectorAdapterBridge {
    pub fn new(connector: Arc<dyn ResourceConnector>) -> Self {
        Self { connector }
    }
}

impl UnifiedProviderAdapter for ConnectorAdapterBridge {
    fn provider_id(&self) -> &str {
        self.connector.provider_id()
    }

    fn capabilities(&self) -> Vec<CapabilityKind> {
        self.connector.capabilities()
    }

    fn submit(
        &self,
        request: &UnifiedRequest,
        credential: &CredentialContext,
    ) -> Result<UnifiedSubmitResult, ProviderError> {
        self.connector.submit(request, credential)
    }

    fn poll(
        &self,
        remote_job_id: &str,
        credential: &CredentialContext,
    ) -> Result<UnifiedPollResult, ProviderError> {
        self.connector.poll(remote_job_id, credential)
    }

    fn download(
        &self,
        result_url: &str,
        target_dir: &Path,
        credential: &CredentialContext,
    ) -> Result<UnifiedDownloadResult, ProviderError> {
        self.connector.download(result_url, target_dir, credential)
    }

    fn health_check(
        &self,
        credential: &CredentialContext,
    ) -> Result<UnifiedHealthStatus, ProviderError> {
        // 将 AccountHealth 映射为 UnifiedHealthStatus
        let health = self.connector.health_check(credential)?;
        Ok(UnifiedHealthStatus {
            available: health.status.is_usable(),
            message: health.message,
            quota_remaining: health.credits_remaining,
        })
    }

    fn is_async(&self) -> bool {
        self.connector.is_async()
    }
}
