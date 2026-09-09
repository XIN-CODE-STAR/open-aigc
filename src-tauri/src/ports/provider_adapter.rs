use crate::domain::providers::{
    GenerationRequest, ProviderError, ProviderHealth, ProviderPollResult, ProviderSubmitResult,
};

/// Provider 适配器端口。每个 AI 供应商实现此 trait。
/// 适配器是阻塞式的（在 spawn_blocking 中调用），使用同步 HTTP 客户端。
#[allow(dead_code)] // Phase 3: provider_id / validate_credential 将在账户型 Provider 落地时使用
pub trait ProviderAdapter: Send {
    /// Provider 唯一标识（如 "openai" / "seedance" / "kling"）。
    fn provider_id(&self) -> &str;

    /// 提交一个生成请求到 Provider，返回远程任务 ID 和初始状态。
    fn submit(
        &mut self,
        base_url: &str,
        api_key: &str,
        model: &str,
        request: &GenerationRequest,
    ) -> Result<ProviderSubmitResult, ProviderError>;

    /// 轮询远程任务状态，返回状态、进度和结果 URL。
    fn poll(
        &mut self,
        base_url: &str,
        api_key: &str,
        remote_task_id: &str,
    ) -> Result<ProviderPollResult, ProviderError>;

    /// 下载结果文件到本地路径。
    fn download_result(
        &mut self,
        base_url: &str,
        api_key: &str,
        result_url: &str,
        local_path: &std::path::Path,
    ) -> Result<(), ProviderError>;

    /// 轻量健康检查（默认实现 ping 一次；账户型 Provider 可 override 为登录态校验）。
    fn validate_credential(
        &self,
        base_url: &str,
        api_key: &str,
    ) -> Result<ProviderHealth, ProviderError> {
        // 默认实现：所有 API Key 模式 provider 假设 key 有效，
        // 实际 health 由上层按需通过 submit 探测。
        let _ = (base_url, api_key);
        Ok(ProviderHealth {
            status: crate::domain::providers::ProviderHealthStatus::Unknown,
            message: None,
            last_checked_at: None,
            refresh_in_seconds: None,
        })
    }
}
