//! 即梦（Jimeng）账号连接器。
//!
//! 通过用户登录态（sessionid Cookie）调用即梦 Web API，
//! 消耗用户自己的会员积分/每日额度进行图片和视频生成。
//!
//! 认证方式：Cookie 中的 sessionid（通过内嵌 WebView 登录获取）。
//! API 基础地址：https://jimeng.jianying.com
//!
//! 与 SeedanceVideoAdapter（API Key → 火山方舟平台）的区别：
//! - 本连接器走即梦 Web 端接口，消耗用户会员权益
//! - SeedanceVideoAdapter 走方舟平台 API，消耗 API 调用额度

use std::path::Path;
use std::time::Duration;

use crate::domain::credentials::CredentialContext;
use crate::domain::providers::ProviderError;
use crate::ports::resource_connector::{
    AccountHealth, AccountStatus, LoginSession, ResourceConnector,
};
use crate::ports::unified_provider::{
    CapabilityKind, UnifiedDownloadResult, UnifiedPollResult, UnifiedRequest, UnifiedSubmitResult,
};

/// 内嵌 jimeng-api 服务默认端口。
/// jimeng-api 本地代理端口（健康检查等跨模块共用）。
pub(crate) const JIMENG_API_PROXY_PORT: u16 = 5100;

/// 即梦 Web API 基础地址。
const JIMENG_BASE_URL: &str = "https://jimeng.jianying.com";

/// 即梦登录页地址（前端 WebView 加载）。
const JIMENG_LOGIN_URL: &str = "https://jimeng.jianying.com/ai-tool/image/generate";

/// 即梦 Web API 版本参数。
const WEB_VERSION: &str = "7.5.0";

/// 默认图片模型。
// 5.0 Lite：即梦 web 当前默认模型（3 积分/张，per_piece 按张计费，支持 gen_count 数量控制）。
const DEFAULT_IMAGE_MODEL: &str = "jimeng-5.0";

/// 默认视频模型（代理期望的格式）。
/// 对外默认视频模型（effective_model / 配置默认值）。
const DEFAULT_VIDEO_MODEL: &str = "seedance-2-0";
/// jimeng-api 代理格式默认视频模型（translate_video_model_for_proxy 回退）。
const DEFAULT_PROXY_VIDEO_MODEL: &str = "jimeng-video-seedance-2.0";

/// 即梦账号连接器配置。
#[derive(Debug, Clone)]
pub struct JimengConfig {
    pub base_url: String,
    pub default_image_model: String,
    pub default_video_model: String,
}

impl Default for JimengConfig {
    fn default() -> Self {
        Self {
            base_url: JIMENG_BASE_URL.into(),
            default_image_model: DEFAULT_IMAGE_MODEL.into(),
            default_video_model: DEFAULT_VIDEO_MODEL.into(),
        }
    }
}

/// 即梦账号连接器（无状态，不持有密钥）。
pub struct JimengConnector {
    config: JimengConfig,
    agent: ureq::Agent,
}

impl JimengConnector {
    pub fn new(config: JimengConfig) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(300))
            .build();
        Self { config, agent }
    }

    /// 从 CredentialContext 提取 sessionid。
    /// 支持两种格式：
    /// 1. cookies 字段中包含完整 cookie 字符串（"sessionid=xxx; other=yyy"）
    /// 2. access_token 字段直接存 sessionid 值
    fn session_id(credential: &CredentialContext) -> Result<String, ProviderError> {
        // 优先从 cookies 中提取 sessionid
        if let Some(cookies) = credential.cookies() {
            if let Some(sid) = extract_sessionid(cookies) {
                return Ok(sid);
            }
            // 如果整个 cookie 字符串本身就是 sessionid（无 key=value 格式）
            if !cookies.contains('=') && !cookies.is_empty() {
                return Ok(cookies.to_owned());
            }
        }
        // 回退到 access_token
        if let Some(token) = credential.access_token() {
            return Ok(token.to_owned());
        }
        Err(ProviderError::ConfigInvalid(
            "即梦 credential 缺少 sessionid（cookies 或 access_token）".into(),
        ))
    }

    /// 有效 base_url。
    fn effective_base_url(&self, credential: &CredentialContext) -> String {
        if credential.base_url.is_empty() {
            self.config.base_url.clone()
        } else {
            credential.base_url.trim_end_matches('/').to_owned()
        }
    }

    /// 有效模型名。
    fn effective_model(&self, request: &UnifiedRequest, credential: &CredentialContext) -> String {
        if !request.model.is_empty() && request.model != "default" {
            return request.model.clone();
        }
        if !credential.model.is_empty() {
            return credential.model.clone();
        }
        match request.capability {
            CapabilityKind::TextToVideo | CapabilityKind::ImageToVideo => {
                self.config.default_video_model.clone()
            }
            _ => self.config.default_image_model.clone(),
        }
    }

    /// 构建带认证头的请求。
    fn authenticated_request(&self, method: &str, url: &str, session_id: &str) -> ureq::Request {
        // session_id 可能是完整 cookie 字符串（含 key=value; 格式）或纯 sessionid 值
        let cookie_header = if session_id.contains('=') {
            session_id.to_owned()
        } else {
            format!("sessionid={session_id}")
        };
        eprintln!("[Jimeng] {method} {url}");
        eprintln!("[Jimeng] Cookie header length: {}", cookie_header.len());
        match method {
            "GET" => self
                .agent
                .get(url)
                .set("Cookie", &cookie_header)
                .set("Accept", "application/json, text/plain, */*")
                .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
                .set("Referer", JIMENG_BASE_URL),
            _ => self
                .agent
                .post(url)
                .set("Cookie", &cookie_header)
                .set("Content-Type", "application/json")
                .set("Accept", "application/json, text/plain, */*")
                .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
                .set("Referer", JIMENG_BASE_URL),
        }
    }

    /// 带签名头的认证请求（即梦原生 API 需要签名验证）。
    fn authenticated_request_with_sign(
        &self,
        method: &str,
        url: &str,
        session_id: &str,
    ) -> ureq::Request {
        let req = self.authenticated_request(method, url, session_id);
        let (device_time, sign) = compute_jimeng_sign(url);
        req.set("Device-Time", &device_time)
            .set("Sign", &sign)
            .set("Sign-Ver", "1")
    }
}

impl ResourceConnector for JimengConnector {
    fn provider_id(&self) -> &str {
        "jimeng"
    }

    fn display_name(&self) -> &str {
        "即梦AI"
    }

    fn capabilities(&self) -> Vec<CapabilityKind> {
        vec![
            CapabilityKind::TextToImage,
            CapabilityKind::ImageToImage,
            CapabilityKind::TextToVideo,
            CapabilityKind::ImageToVideo,
        ]
    }

    fn login_url(&self) -> &str {
        JIMENG_LOGIN_URL
    }

    fn validate_session(&self, session: &LoginSession) -> Result<AccountHealth, ProviderError> {
        // 用 session 调用用户信息接口验证有效性
        let session_id = extract_sessionid(&session.cookies)
            .or_else(|| {
                if !session.cookies.contains('=') && !session.cookies.is_empty() {
                    Some(session.cookies.clone())
                } else {
                    None
                }
            })
            .or_else(|| session.access_token.clone())
            .ok_or_else(|| ProviderError::ConfigInvalid("登录会话中未找到 sessionid".into()))?;

        let base_url = self.config.base_url.trim_end_matches('/');
        let url = format!("{base_url}/web/api/media/user/info/");
        let resp: serde_json::Value = self
            .authenticated_request("GET", &url, &session_id)
            .call()
            .map_err(classify_jimeng_error)?
            .into_json()
            .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

        // 即梦 API 响应格式：{ "status_code": 0, "data": { ... } }
        let status_code = resp["status_code"].as_i64().unwrap_or(-1);
        if status_code != 0 {
            let msg = resp["status_msg"].as_str().unwrap_or("session 无效");
            return Ok(AccountHealth {
                status: AccountStatus::NeedLogin,
                message: msg.to_owned(),
                credits_remaining: None,
                membership: None,
            });
        }

        let nickname = resp["data"]["nickname"].as_str().unwrap_or("即梦用户");
        Ok(AccountHealth {
            status: AccountStatus::Active,
            message: format!("已登录：{nickname}"),
            credits_remaining: None,
            membership: Some(nickname.to_owned()),
        })
    }

    fn health_check(&self, credential: &CredentialContext) -> Result<AccountHealth, ProviderError> {
        let session_id = Self::session_id(credential)?;
        let base_url = self.effective_base_url(credential);
        let url = format!("{base_url}/web/api/media/user/info/");

        let result = self.authenticated_request("GET", &url, &session_id).call();

        match result {
            Ok(response) => {
                let resp: serde_json::Value = response
                    .into_json()
                    .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;
                let status_code = resp["status_code"].as_i64().unwrap_or(-1);
                if status_code == 0 {
                    let nickname = resp["data"]["nickname"].as_str().unwrap_or("即梦用户");
                    Ok(AccountHealth {
                        status: AccountStatus::Active,
                        message: format!("已登录：{nickname}"),
                        credits_remaining: None,
                        membership: Some(nickname.to_owned()),
                    })
                } else {
                    Ok(AccountHealth {
                        status: AccountStatus::NeedLogin,
                        message: "登录态已失效，请重新登录".to_owned(),
                        credits_remaining: None,
                        membership: None,
                    })
                }
            }
            Err(e) => Ok(AccountHealth {
                status: AccountStatus::Unknown,
                message: format!("连接失败: {e}"),
                credits_remaining: None,
                membership: None,
            }),
        }
    }

    fn submit(
        &self,
        request: &UnifiedRequest,
        credential: &CredentialContext,
    ) -> Result<UnifiedSubmitResult, ProviderError> {
        let session_id = Self::session_id(credential)?;
        let base_url = self.effective_base_url(credential);
        let model = self.effective_model(request, credential);

        match request.capability {
            CapabilityKind::TextToImage | CapabilityKind::ImageToImage => {
                self.submit_image(request, &base_url, &model, &session_id)
            }
            CapabilityKind::TextToVideo | CapabilityKind::ImageToVideo => {
                self.submit_video(request, &base_url, &model, &session_id)
            }
            _ => Err(ProviderError::ConfigInvalid(format!(
                "即梦不支持的能力：{:?}",
                request.capability
            ))),
        }
    }

    fn poll(
        &self,
        remote_job_id: &str,
        credential: &CredentialContext,
    ) -> Result<UnifiedPollResult, ProviderError> {
        let session_id = Self::session_id(credential)?;
        let base_url = self.effective_base_url(credential);

        // remote_job_id 格式："{kind}:{task_id}"
        let (kind, task_id) = remote_job_id
            .split_once(':')
            .unwrap_or(("video", remote_job_id));

        match kind {
            "image" | "proxy" => {
                // 图片/代理视频生成是同步的，poll 不应被调用；但如果被调用，返回成功
                // task_id 对于 proxy:video 格式可能是 "video:{url}"
                let result_url = if task_id.starts_with("video:") {
                    task_id.strip_prefix("video:").unwrap_or(task_id).to_owned()
                } else {
                    task_id.to_owned()
                };
                Ok(UnifiedPollResult {
                    status: "succeeded".to_owned(),
                    progress: 100,
                    result_url: Some(result_url),
                    error_message: None,
                    retryable: false,
                })
            }
            _ => {
                // 视频轮询
                let url = format!(
                    "{base_url}/web/api/v2/video/generate/query/?task_id={task_id}&web_version={WEB_VERSION}"
                );
                let resp: serde_json::Value = self
                    .authenticated_request_with_sign("GET", &url, &session_id)
                    .call()
                    .map_err(classify_jimeng_error)?
                    .into_json()
                    .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

                let status_code = resp["status_code"].as_i64().unwrap_or(-1);
                if status_code != 0 {
                    let msg = resp["status_msg"].as_str().unwrap_or("查询失败");
                    return Ok(UnifiedPollResult {
                        status: "failed".to_owned(),
                        progress: 0,
                        result_url: None,
                        error_message: Some(msg.to_owned()),
                        retryable: true,
                    });
                }

                // 即梦视频任务状态：20/42/45 = 处理中，其他成功码 = 完成
                let task_status = resp["data"]["status"].as_i64().unwrap_or(0);
                match task_status {
                    20 | 42 | 45 => Ok(UnifiedPollResult {
                        status: "processing".to_owned(),
                        progress: 50,
                        result_url: None,
                        error_message: None,
                        retryable: false,
                    }),
                    _ => {
                        // 尝试提取视频 URL
                        let video_url = resp["data"]["video_url"]
                            .as_str()
                            .or_else(|| {
                                resp["data"]["result"]
                                    .as_array()
                                    .and_then(|arr| arr.first())
                                    .and_then(|item| item["url"].as_str())
                            })
                            .map(|s| s.to_owned());

                        if video_url.is_some() {
                            Ok(UnifiedPollResult {
                                status: "succeeded".to_owned(),
                                progress: 100,
                                result_url: video_url,
                                error_message: None,
                                retryable: false,
                            })
                        } else if task_status == 0 {
                            // 仍在排队
                            Ok(UnifiedPollResult {
                                status: "processing".to_owned(),
                                progress: 10,
                                result_url: None,
                                error_message: None,
                                retryable: false,
                            })
                        } else {
                            Ok(UnifiedPollResult {
                                status: "failed".to_owned(),
                                progress: 0,
                                result_url: None,
                                error_message: Some("任务完成但未返回视频 URL".to_owned()),
                                retryable: true,
                            })
                        }
                    }
                }
            }
        }
    }

    fn download(
        &self,
        result_url: &str,
        target_dir: &Path,
        _credential: &CredentialContext,
    ) -> Result<UnifiedDownloadResult, ProviderError> {
        // 即梦返回的 URL 通常自带签名，无需额外认证
        let resp = self
            .agent
            .get(result_url)
            .call()
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let content_type = resp
            .header("Content-Type")
            .unwrap_or("image/webp")
            .to_owned();
        let extension = if content_type.contains("video") {
            "mp4"
        } else if content_type.contains("png") {
            "png"
        } else {
            "webp"
        };
        let file_name = format!("jimeng-{}.{}", uuid::Uuid::new_v4(), extension);
        let file_path = target_dir.join(&file_name);

        let mut body = resp.into_reader();
        let mut file =
            std::fs::File::create(&file_path).map_err(|e| ProviderError::Network(e.to_string()))?;
        std::io::copy(&mut body, &mut file).map_err(|e| ProviderError::Network(e.to_string()))?;

        let file_size = std::fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0);
        let mime_type = match extension {
            "mp4" => "video/mp4",
            "png" => "image/png",
            _ => "image/webp",
        };

        // 视频文件提取元数据（时长、分辨率）
        let (duration_secs, width, height) = if extension == "mp4" {
            let meta = parse_mp4_metadata(&file_path);
            eprintln!(
                "[Jimeng] video metadata: duration={:?}s, {:?}x{:?}",
                meta.0, meta.1, meta.2
            );
            meta
        } else {
            (None, None, None)
        };

        Ok(UnifiedDownloadResult {
            file_path: file_path.to_string_lossy().to_string(),
            mime_type: mime_type.to_owned(),
            file_size,
            duration_secs,
            width,
            height,
        })
    }

    fn is_async(&self) -> bool {
        true
    }
}

// ── 内部方法 ──

impl JimengConnector {
    /// 提交图片生成（同步返回结果 URL）。
    /// 优先通过内嵌 jimeng-api 代理（处理签名），仅在代理不可达时回退到原生 API。
    fn submit_image(
        &self,
        request: &UnifiedRequest,
        base_url: &str,
        model: &str,
        session_id: &str,
    ) -> Result<UnifiedSubmitResult, ProviderError> {
        // 提取纯 sessionid（去掉 "sessionid=" 前缀或完整 cookie 格式）
        let pure_session_id = extract_pure_session_id(session_id);

        // 图生图：对话参考图通常是 data: URL，只有代理的 compositions 端点
        // 能接收并上传；原生 API 需要公网图片 URL，因此 i2i 不做原生回退。
        if request.capability == CapabilityKind::ImageToImage {
            if let Some(reference) = &request.reference_image_url {
                return self.submit_image_i2i_via_proxy(
                    request,
                    &pure_session_id,
                    reference,
                    model,
                );
            }
        }

        // 尝试通过 jimeng-api 代理调用（OpenAI 兼容格式，自动处理签名）
        match self.submit_via_proxy(request, &pure_session_id, model) {
            Ok(result) => Ok(result),
            Err(ProviderError::Network(_)) => {
                // 代理不可达，回退到原生 API
                eprintln!("[Jimeng] proxy unreachable, falling back to native API");
                self.submit_via_native(request, base_url, model, session_id)
            }
            Err(e) => {
                // 代理返回了业务错误（登录失效、限流等），直接返回，不回退
                // （原生 API 使用相同的 session，也会失败）
                eprintln!("[Jimeng] proxy returned error, NOT falling back to native: {e}");
                Err(e)
            }
        }
    }

    /// 通过 jimeng-api 代理的 /v1/images/compositions 端点做图生图（同步返回）。
    fn submit_image_i2i_via_proxy(
        &self,
        request: &UnifiedRequest,
        session_id: &str,
        reference_image: &str,
        model: &str,
    ) -> Result<UnifiedSubmitResult, ProviderError> {
        let url = format!("http://127.0.0.1:{JIMENG_API_PROXY_PORT}/v1/images/compositions");
        let ratio = request
            .parameters
            .get("ratio")
            .and_then(|v| v.as_str())
            .unwrap_or("1:1");
        let resolution = request
            .parameters
            .get("resolution")
            .and_then(|v| v.as_str())
            .unwrap_or("2k");
        let body = serde_json::json!({
            "model": model,
            "prompt": request.prompt,
            "images": [reference_image],
            "ratio": ratio,
            "resolution": resolution,
            "sample_strength": 0.5,
            "count": 1,
        });
        eprintln!(
            "[Jimeng] i2i proxy request: model={model}, ratio={ratio}, resolution={resolution}, ref_len={}",
            reference_image.len()
        );
        let resp_raw = self
            .agent
            .post(&url)
            .set("Authorization", &format!("Bearer {session_id}"))
            .set("Content-Type", "application/json")
            .set("Accept", "application/json")
            .send_json(&body);
        let resp = match resp_raw {
            Ok(resp) => resp,
            Err(e) => {
                eprintln!("[Jimeng] i2i proxy request failed: {e}");
                return Err(ProviderError::Network(format!(
                    "jimeng-api proxy unreachable: {e}"
                )));
            }
        };
        let status = resp.status();
        let body_text = resp.into_string().unwrap_or_default();
        eprintln!(
            "[Jimeng] i2i proxy response HTTP {status}, len={}",
            body_text.len()
        );
        if status >= 400 {
            return Err(ProviderError::Remote {
                status: status as u16,
                body: body_text[..body_text.len().min(300)].to_owned(),
            });
        }
        let json: serde_json::Value = serde_json::from_str(&body_text)
            .map_err(|e| ProviderError::MalformedResponse(format!("proxy JSON error: {e}")))?;
        if let Some(code) = json.get("code").and_then(|v| v.as_i64()) {
            if code != 0 {
                let msg = json
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("代理返回错误");
                eprintln!("[Jimeng] i2i proxy error: code={code}, msg={msg}");
                return Err(ProviderError::Remote {
                    status: status as u16,
                    body: msg.to_owned(),
                });
            }
        }
        let first_url = json["data"]
            .as_array()
            .and_then(|items| items.first())
            .and_then(|item| item["url"].as_str())
            .ok_or_else(|| {
                ProviderError::MalformedResponse("jimeng-api 图生图未返回图片 URL".to_owned())
            })?;
        eprintln!("[Jimeng] i2i generated 1 image");
        Ok(UnifiedSubmitResult {
            remote_job_id: format!("proxy:i2i:{first_url}"),
            initial_status: "succeeded".to_owned(),
            immediate_result_url: Some(first_url.to_owned()),
            estimated_duration_secs: None,
        })
    }

    /// 通过 jimeng-api 代理提交图片生成（OpenAI 兼容格式）。
    fn submit_via_proxy(
        &self,
        request: &UnifiedRequest,
        session_id: &str,
        model: &str,
    ) -> Result<UnifiedSubmitResult, ProviderError> {
        let url = format!("http://127.0.0.1:{JIMENG_API_PROXY_PORT}/v1/images/generations");

        // 从 request.parameters 读取参数
        let ratio = request
            .parameters
            .get("ratio")
            .and_then(|v| v.as_str())
            .unwrap_or("1:1");
        let resolution = request
            .parameters
            .get("resolution")
            .and_then(|v| v.as_str())
            .unwrap_or("2k");
        let count = request
            .parameters
            .get("count")
            .and_then(|v| v.as_i64())
            .unwrap_or(1)
            .clamp(1, 4) as u32;
        let intelligent_ratio = request
            .parameters
            .get("intelligent_ratio")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let sample_strength = request
            .parameters
            .get("sample_strength")
            .and_then(|v| v.as_f64());

        // jimeng-api 不支持 n 参数，多图需循环调用
        let mut all_urls: Vec<String> = Vec::new();
        for i in 0..count {
            eprintln!(
                "[Jimeng] proxy request {}/{count}: ratio={ratio}, resolution={resolution}",
                i + 1
            );

            let mut body = serde_json::json!({
                "model": model,
                "prompt": request.prompt,
                "ratio": ratio,
                "resolution": resolution,
                "intelligent_ratio": intelligent_ratio,
                "count": count,
            });

            if let Some(ref neg) = request.negative_prompt {
                body["negative_prompt"] = serde_json::json!(neg);
            }
            if let Some(strength) = sample_strength {
                body["sample_strength"] = serde_json::json!(strength);
            }

            let resp_raw = self
                .agent
                .post(&url)
                .set("Authorization", &format!("Bearer {session_id}"))
                .set("Content-Type", "application/json")
                .set("Accept", "application/json")
                .send_json(&body);

            match resp_raw {
                Ok(resp) => {
                    let status = resp.status();
                    let body_text = resp.into_string().unwrap_or_default();
                    eprintln!(
                        "[Jimeng] proxy response HTTP {status}, len={}",
                        body_text.len()
                    );

                    if status >= 400 {
                        return Err(ProviderError::Remote {
                            status,
                            body: body_text[..body_text.len().min(300)].to_owned(),
                        });
                    }

                    let json: serde_json::Value =
                        serde_json::from_str(&body_text).map_err(|e| {
                            ProviderError::MalformedResponse(format!("proxy JSON error: {e}"))
                        })?;

                    // 检测代理错误响应（HTTP 200 但 body 包含错误码）
                    if let Some(code) = json.get("code").and_then(|v| v.as_i64()) {
                        if code != 0 {
                            let msg = json
                                .get("message")
                                .and_then(|v| v.as_str())
                                .unwrap_or("代理返回错误");
                            eprintln!("[Jimeng] proxy error: code={code}, msg={msg}");
                            // 登录失效等错误不应 fallback 到原生 API（原生也会失败）
                            return Err(ProviderError::Remote {
                                status,
                                body: msg.to_owned(),
                            });
                        }
                    }

                    // 提取所有图片 URL
                    if let Some(data_arr) = json["data"].as_array() {
                        for item in data_arr {
                            if let Some(img_url) = item["url"].as_str() {
                                all_urls.push(img_url.to_owned());
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[Jimeng] proxy request failed: {e}");
                    return Err(ProviderError::Network(format!(
                        "jimeng-api proxy unreachable: {e}"
                    )));
                }
            }
        }

        if all_urls.is_empty() {
            return Err(ProviderError::MalformedResponse(
                "jimeng-api 未返回任何图片 URL".to_owned(),
            ));
        }

        eprintln!("[Jimeng] generated {} image(s)", all_urls.len());

        // 返回第一张图片的 URL（其余图片在 resultJson 中可查看）
        Ok(UnifiedSubmitResult {
            remote_job_id: format!("proxy:image:{}", all_urls[0]),
            initial_status: "succeeded".to_owned(),
            immediate_result_url: Some(all_urls[0].clone()),
            estimated_duration_secs: None,
        })
    }

    /// 直接调用即梦原生 API（需要完整 cookie，可能被签名拦截）。
    fn submit_via_native(
        &self,
        request: &UnifiedRequest,
        base_url: &str,
        model: &str,
        session_id: &str,
    ) -> Result<UnifiedSubmitResult, ProviderError> {
        let url = format!("{base_url}/api/v1/aigc/img/generate?web_version={WEB_VERSION}");

        let mut body = serde_json::json!({
            "model_req_key": model,
            "prompt": request.prompt,
            "width": request.parameters.get("width").and_then(|v| v.as_i64()).unwrap_or(2048),
            "height": request.parameters.get("height").and_then(|v| v.as_i64()).unwrap_or(2048),
            // 数量控制由 jimeng-api 代理的 draft gen_option.gen_count 完成；
            // 原生 v1 接口为无签名回退路径，不支持数量参数（缺省 4 张）。
        });

        if let Some(negative) = &request.negative_prompt {
            body["negative_prompt"] = serde_json::json!(negative);
        }

        // 图生图：附加参考图片
        if request.capability == CapabilityKind::ImageToImage {
            if let Some(image_url) = &request.reference_image_url {
                body["image_urls"] = serde_json::json!([image_url]);
            }
        }

        if let Some(seed) = request.parameters.get("seed") {
            body["seed"] = seed.clone();
        }

        eprintln!("[Jimeng] submit_via_native: url={url}, model={model}, prompt_len={}, session_id_len={}", request.prompt.len(), session_id.len());

        let resp_raw = self
            .authenticated_request("POST", &url, session_id)
            .send_json(&body)
            .map_err(classify_jimeng_error)?;

        let status = resp_raw.status();
        let body_text = resp_raw.into_string().unwrap_or_default();
        eprintln!(
            "[Jimeng] submit_image HTTP {status}, body_len={}, preview={}",
            body_text.len(),
            &body_text[..body_text.len().min(300)]
        );

        let resp: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
            ProviderError::MalformedResponse(format!(
                "Failed to read JSON: {e}. HTTP {status}, body={}",
                &body_text[..body_text.len().min(200)]
            ))
        })?;

        let status_code = resp["status_code"].as_i64().unwrap_or(-1);
        if status_code != 0 {
            let msg = resp["status_msg"].as_str().unwrap_or("图片生成失败");
            return Err(ProviderError::Remote {
                status: status_code as u16,
                body: msg.to_owned(),
            });
        }

        // 即梦图片生成通常是同步的，直接返回 URL
        let image_url = resp["data"]["image_urls"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .or_else(|| resp["data"]["url"].as_str())
            .map(|s| s.to_owned());

        match image_url {
            Some(url) => Ok(UnifiedSubmitResult {
                remote_job_id: format!("image:{url}"),
                initial_status: "succeeded".to_owned(),
                immediate_result_url: Some(url),
                estimated_duration_secs: None,
            }),
            None => {
                // 可能是异步的，返回 task_id
                let task_id = resp["data"]["task_id"].as_str().unwrap_or("");
                Ok(UnifiedSubmitResult {
                    remote_job_id: format!("image:{task_id}"),
                    initial_status: "processing".to_owned(),
                    immediate_result_url: None,
                    estimated_duration_secs: Some(30),
                })
            }
        }
    }

    /// 提交视频生成。优先通过代理（处理签名 + 轮询），回退到原生 API。
    fn submit_video(
        &self,
        request: &UnifiedRequest,
        base_url: &str,
        model: &str,
        session_id: &str,
    ) -> Result<UnifiedSubmitResult, ProviderError> {
        let pure_session_id = extract_pure_session_id(session_id);

        // 尝试通过 jimeng-api 代理提交视频生成
        match self.submit_video_via_proxy(request, &pure_session_id, model) {
            Ok(result) => Ok(result),
            Err(ProviderError::Network(_)) => {
                eprintln!("[Jimeng] proxy unreachable for video, falling back to native API");
                self.submit_video_native(request, base_url, model, session_id)
            }
            Err(e) => {
                eprintln!("[Jimeng] proxy video error, NOT falling back: {e}");
                Err(e)
            }
        }
    }

    /// 通过 jimeng-api 代理提交视频生成（代理内部处理签名和轮询）。
    fn submit_video_via_proxy(
        &self,
        request: &UnifiedRequest,
        session_id: &str,
        model: &str,
    ) -> Result<UnifiedSubmitResult, ProviderError> {
        let url = format!("http://127.0.0.1:{JIMENG_API_PROXY_PORT}/v1/videos/generations");

        let ratio = request
            .parameters
            .get("ratio")
            .or_else(|| request.parameters.get("aspect_ratio"))
            .and_then(|v| v.as_str())
            .unwrap_or("16:9");
        let resolution = request
            .parameters
            .get("resolution")
            .and_then(|v| v.as_str())
            .unwrap_or("720p");
        let duration = request
            .parameters
            .get("duration")
            .and_then(|v| v.as_i64())
            .or_else(|| {
                // 处理字符串格式如 "4s" → 4
                request
                    .parameters
                    .get("duration")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.trim_end_matches('s').parse::<i64>().ok())
            })
            .unwrap_or(5);

        // 翻译模型名为代理期望的格式
        let proxy_model = translate_video_model_for_proxy(model);

        let mut body = serde_json::json!({
            "model": proxy_model,
            "prompt": request.prompt,
            "ratio": ratio,
            "resolution": resolution,
            "duration": duration,
        });

        // 图生视频：附加首帧图片
        if let Some(image_url) = &request.reference_image_url {
            body["image_url"] = serde_json::json!(image_url);
        }

        eprintln!("[Jimeng] proxy video request: model={proxy_model} (input={model}), ratio={ratio}, resolution={resolution}, duration={duration}");

        let resp_raw = self
            .agent
            .post(&url)
            .set("Authorization", &format!("Bearer {session_id}"))
            .set("Content-Type", "application/json")
            .set("Accept", "application/json")
            .timeout(std::time::Duration::from_secs(1800)) // 视频生成可能需要较长时间（代理轮询最长 60 分钟）
            .send_json(&body);

        match resp_raw {
            Ok(resp) => {
                let status = resp.status();
                let body_text = resp.into_string().unwrap_or_default();
                eprintln!(
                    "[Jimeng] proxy video response HTTP {status}, len={}",
                    body_text.len()
                );

                if status >= 400 {
                    return Err(ProviderError::Remote {
                        status,
                        body: body_text[..body_text.len().min(300)].to_owned(),
                    });
                }

                let json: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
                    ProviderError::MalformedResponse(format!("proxy video JSON error: {e}"))
                })?;

                // 检测代理错误响应
                if let Some(code) = json.get("code").and_then(|v| v.as_i64()) {
                    if code != 0 {
                        let msg = json
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("代理返回错误");
                        return Err(ProviderError::Remote {
                            status,
                            body: msg.to_owned(),
                        });
                    }
                }

                // 提取视频 URL（代理同步返回最终结果）
                let video_url = json["data"]
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|item| item["url"].as_str())
                    .ok_or_else(|| ProviderError::MalformedResponse("代理未返回视频 URL".into()))?;

                eprintln!(
                    "[Jimeng] proxy video generated: {}",
                    &video_url[..video_url.len().min(80)]
                );

                Ok(UnifiedSubmitResult {
                    remote_job_id: format!("proxy:video:{video_url}"),
                    initial_status: "succeeded".to_owned(),
                    immediate_result_url: Some(video_url.to_owned()),
                    estimated_duration_secs: None,
                })
            }
            Err(e) => {
                eprintln!("[Jimeng] proxy video request failed: {e}");
                Err(ProviderError::Network(format!(
                    "jimeng-api proxy unreachable: {e}"
                )))
            }
        }
    }

    /// 直接调用即梦原生视频 API（需要签名头，可能失败）。
    fn submit_video_native(
        &self,
        request: &UnifiedRequest,
        base_url: &str,
        model: &str,
        session_id: &str,
    ) -> Result<UnifiedSubmitResult, ProviderError> {
        let url = format!("{base_url}/web/api/v2/video/generate/submit/?web_version={WEB_VERSION}");

        let mut body = serde_json::json!({
            "model_req_key": model,
            "prompt": request.prompt,
        });

        // 视频参数
        if let Some(duration) = request.parameters.get("duration") {
            body["duration"] = duration.clone();
        }
        if let Some(ratio) = request.parameters.get("ratio") {
            body["ratio"] = ratio.clone();
        } else if let Some(aspect_ratio) = request.parameters.get("aspect_ratio") {
            body["ratio"] = aspect_ratio.clone();
        }
        if let Some(resolution) = request.parameters.get("resolution") {
            body["resolution"] = resolution.clone();
        }

        // 图生视频：附加首帧图片
        if request.capability == CapabilityKind::ImageToVideo {
            if let Some(image_url) = &request.reference_image_url {
                body["image_url"] = serde_json::json!(image_url);
            }
        }

        let http_resp = self
            .authenticated_request_with_sign("POST", &url, session_id)
            .send_json(&body)
            .map_err(classify_jimeng_error)?;

        // 检测 HTML 响应（通常意味着签名缺失或 session 失效导致重定向到登录页）
        let content_type = http_resp.header("Content-Type").unwrap_or("").to_owned();
        if content_type.contains("text/html") {
            let preview = http_resp.into_string().unwrap_or_default();
            let preview_short = &preview[..preview.len().min(200)];
            eprintln!(
                "[Jimeng] native video API returned HTML (likely auth redirect): {preview_short}"
            );
            return Err(ProviderError::ConfigInvalid(
                "即梦视频 API 返回 HTML（session 失效或签名缺失），请重新登录".into(),
            ));
        }

        let resp: serde_json::Value = http_resp
            .into_json()
            .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

        let status_code = resp["status_code"].as_i64().unwrap_or(-1);
        if status_code != 0 {
            let msg = resp["status_msg"].as_str().unwrap_or("视频生成提交失败");
            return Err(ProviderError::Remote {
                status: status_code as u16,
                body: msg.to_owned(),
            });
        }

        let task_id = resp["data"]["task_id"]
            .as_str()
            .or_else(|| resp["data"]["id"].as_str())
            .ok_or_else(|| ProviderError::MalformedResponse("响应缺少 task_id".into()))?;

        Ok(UnifiedSubmitResult {
            remote_job_id: format!("video:{task_id}"),
            initial_status: "queued".to_owned(),
            immediate_result_url: None,
            estimated_duration_secs: Some(180),
        })
    }
}

// ── 工具函数 ──

/// 从 Cookie 字符串中提取 sessionid 值。
fn extract_sessionid(cookies: &str) -> Option<String> {
    cookies.split(';').find_map(|pair| {
        let pair = pair.trim();
        if let Some((key, value)) = pair.split_once('=') {
            if key.trim().eq_ignore_ascii_case("sessionid") {
                return Some(value.trim().to_owned());
            }
        }
        None
    })
}

fn classify_jimeng_error(error: ureq::Error) -> ProviderError {
    let message = error.to_string();
    match error {
        ureq::Error::Status(status, response) => {
            let body = response.into_string().unwrap_or_default();
            eprintln!(
                "[Jimeng] HTTP error {status}, body_len={}, preview={}",
                body.len(),
                &body[..body.len().min(500)]
            );
            // 401/403 通常意味着 session 过期
            if status == 401 || status == 403 {
                ProviderError::ConfigInvalid(format!("登录态失效（HTTP {status}）: {body}"))
            } else {
                ProviderError::Remote { status, body }
            }
        }
        _ => {
            eprintln!("[Jimeng] network error: {message}");
            ProviderError::Network(message)
        }
    }
}

/// 从完整 cookie 字符串或纯值中提取纯 sessionid。
/// 支持格式：
/// - "sessionid=xxx; other=yyy" → "xxx"
/// - "sessionid=xxx" → "xxx"
/// - "xxx" (纯值) → "xxx"
fn extract_pure_session_id(raw: &str) -> String {
    // 尝试从 cookie 字符串中提取 sessionid=xxx
    for part in raw.split(';') {
        let part = part.trim();
        if let Some(val) = part.strip_prefix("sessionid=") {
            let val = val.trim();
            if !val.is_empty() {
                return val.to_owned();
            }
        }
    }
    // 回退：原样返回
    raw.trim().to_owned()
}

/// 将连接器内部视频模型名翻译为 jimeng-api 代理期望的格式。
///
/// 代理 VIDEO_MODEL_MAP 键格式为 `"jimeng-video-*"`，而连接器内部可能使用
/// `"seedance-2-0"` 等简写形式。
fn translate_video_model_for_proxy(model: &str) -> String {
    match model {
        // 已经是代理格式
        m if m.starts_with("jimeng-video-") => m.to_owned(),
        // 简写形式 → 代理格式
        "seedance-2-0" | "seedance-2.0" => "jimeng-video-seedance-2.0".into(),
        "seedance-2-0-fast" | "seedance-2.0-fast" => "jimeng-video-seedance-2.0-fast".into(),
        "3.5-pro" | "vgfm-3.5-pro" => "jimeng-video-3.5-pro".into(),
        "3.0-pro" | "vgfm-3.0-pro" => "jimeng-video-3.0-pro".into(),
        "3.0" | "vgfm-3.0" => "jimeng-video-3.0".into(),
        "3.0-fast" | "vgfm-3.0-fast" => "jimeng-video-3.0-fast".into(),
        "2.0" | "vgfm-lite" => "jimeng-video-2.0".into(),
        "2.0-pro" | "vgfm1.0" => "jimeng-video-2.0-pro".into(),
        // 未知模型 → 使用默认
        _ => {
            eprintln!(
                "[Jimeng] unknown video model '{model}', using default {DEFAULT_PROXY_VIDEO_MODEL}"
            );
            DEFAULT_PROXY_VIDEO_MODEL.into()
        }
    }
}

/// 从 MP4 文件头部解析基础元数据（时长、宽高）。
///
/// 解析 ISO Base Media File Format 的 moov/mvhd/tkhd box。
/// 返回 (duration_secs, width, height)，解析失败时返回 None。
fn parse_mp4_metadata(path: &Path) -> (Option<f64>, Option<u32>, Option<u32>) {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return (None, None, None),
    };

    // 查找 moov box
    let moov_offset = find_box(&data, b"moov");
    let moov_offset = match moov_offset {
        Some(o) => o,
        None => return (None, None, None),
    };

    let moov_size = read_u32(&data, moov_offset).unwrap_or(0) as usize;
    let moov_end = moov_offset + moov_size;
    let moov_data = &data[moov_offset..moov_end.min(data.len())];

    // 在 moov 内查找 mvhd box 获取时长
    let duration_secs = if let Some(mvhd_rel) = find_box(&moov_data[8..], b"mvhd") {
        let mvhd_off = 8 + mvhd_rel + 8; // skip box header (size + type)
        let version = moov_data.get(mvhd_off).copied().unwrap_or(0);
        let (timescale, duration) = if version == 0 {
            let ts = read_u32(moov_data, mvhd_off + 12).unwrap_or(1);
            let dur = read_u32(moov_data, mvhd_off + 16).unwrap_or(0);
            (ts, dur as u64)
        } else {
            // version 1: 64-bit timestamps
            let ts = read_u32(moov_data, mvhd_off + 20).unwrap_or(1);
            let dur = read_u64(moov_data, mvhd_off + 24).unwrap_or(0);
            (ts, dur)
        };
        if timescale > 0 {
            Some(duration as f64 / timescale as f64)
        } else {
            None
        }
    } else {
        None
    };

    // 在 moov 内查找 trak → tkhd 获取宽高
    let (width, height) = if let Some(trak_rel) = find_box(&moov_data[8..], b"trak") {
        let trak_off = 8 + trak_rel;
        let trak_size = read_u32(moov_data, trak_off).unwrap_or(0) as usize;
        let trak_data = &moov_data[trak_off..(trak_off + trak_size).min(moov_data.len())];
        if let Some(tkhd_rel) = find_box(&trak_data[8..], b"tkhd") {
            let tkhd_off = 8 + tkhd_rel + 8;
            let version = trak_data.get(tkhd_off).copied().unwrap_or(0);
            // width/height are at the end of tkhd (fixed-point 16.16)
            let wh_offset = if version == 0 { 76 } else { 84 };
            let w_raw = read_u32(trak_data, tkhd_off + wh_offset).unwrap_or(0);
            let h_raw = read_u32(trak_data, tkhd_off + wh_offset + 4).unwrap_or(0);
            let w = (w_raw >> 16) as u32;
            let h = (h_raw >> 16) as u32;
            if w > 0 && h > 0 {
                (Some(w), Some(h))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    (duration_secs, width, height)
}

/// 在数据中查找指定类型的 box（从偏移 0 开始扫描顶层 box）。
/// 返回 box 起始偏移（包含 4 字节 size + 4 字节 type）。
fn find_box(data: &[u8], box_type: &[u8; 4]) -> Option<usize> {
    let mut pos = 0;
    while pos + 8 <= data.len() {
        let size = read_u32(data, pos).unwrap_or(0) as usize;
        if size < 8 || pos + size > data.len() {
            break;
        }
        if &data[pos + 4..pos + 8] == box_type {
            return Some(pos);
        }
        pos += size;
    }
    None
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    if offset + 4 > data.len() {
        return None;
    }
    Some(u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

fn read_u64(data: &[u8], offset: usize) -> Option<u64> {
    if offset + 8 > data.len() {
        return None;
    }
    Some(u64::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
        data[offset + 4],
        data[offset + 5],
        data[offset + 6],
        data[offset + 7],
    ]))
}

/// 即梦 API 签名计算。
/// 签名算法：md5("9e2c|" + uri_tail + "|7|8.4.0|" + timestamp + "||11ac")
/// 其中 uri_tail = URL 路径的最后 7 个字符。
fn compute_jimeng_sign(url: &str) -> (String, String) {
    use md5::{Digest, Md5};

    let device_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();

    // 提取 URI 路径的最后 7 个字符
    let path = url.split('?').next().unwrap_or("");
    let uri_tail: String = if path.len() >= 7 {
        path[path.len() - 7..].to_owned()
    } else {
        path.to_owned()
    };

    let sign_input = format!("9e2c|{uri_tail}|7|8.4.0|{device_time}||11ac");
    let mut hasher = Md5::new();
    hasher.update(sign_input.as_bytes());
    let sign = format!("{:x}", hasher.finalize());

    (device_time, sign)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::credentials::CredentialType;

    fn test_credential() -> CredentialContext {
        CredentialContext {
            provider_id: "jimeng".into(),
            credential_type: CredentialType::SessionCookie,
            payload: serde_json::json!({
                "cookies": "sessionid=test-session-123; uid=456",
                "access_token": null
            }),
            base_url: String::new(),
            model: String::new(),
        }
    }

    #[test]
    fn jimeng_connector_metadata() {
        let connector = JimengConnector::new(JimengConfig::default());
        assert_eq!(connector.provider_id(), "jimeng");
        assert_eq!(connector.display_name(), "即梦AI");
        assert!(connector.is_async());
        let caps = connector.capabilities();
        assert!(caps.contains(&CapabilityKind::TextToImage));
        assert!(caps.contains(&CapabilityKind::TextToVideo));
        assert!(caps.contains(&CapabilityKind::ImageToVideo));
        assert!(caps.contains(&CapabilityKind::ImageToImage));
    }

    #[test]
    fn jimeng_session_id_extraction() {
        let credential = test_credential();
        let sid = JimengConnector::session_id(&credential).unwrap();
        assert_eq!(sid, "test-session-123");
    }

    #[test]
    fn jimeng_session_id_from_access_token() {
        let credential = CredentialContext {
            provider_id: "jimeng".into(),
            credential_type: CredentialType::SessionCookie,
            payload: serde_json::json!({
                "access_token": "direct-session-id"
            }),
            base_url: String::new(),
            model: String::new(),
        };
        let sid = JimengConnector::session_id(&credential).unwrap();
        assert_eq!(sid, "direct-session-id");
    }

    #[test]
    fn jimeng_session_id_missing() {
        let credential = CredentialContext {
            provider_id: "jimeng".into(),
            credential_type: CredentialType::SessionCookie,
            payload: serde_json::json!({}),
            base_url: String::new(),
            model: String::new(),
        };
        assert!(JimengConnector::session_id(&credential).is_err());
    }

    #[test]
    fn jimeng_extract_sessionid_helper() {
        assert_eq!(
            extract_sessionid("sessionid=abc123; uid=456; token=xyz"),
            Some("abc123".to_owned())
        );
        assert_eq!(extract_sessionid("uid=456; token=xyz"), None);
        assert_eq!(
            extract_sessionid("SessionID=case-insensitive"),
            Some("case-insensitive".to_owned())
        );
    }

    #[test]
    fn jimeng_effective_model_fallback() {
        let connector = JimengConnector::new(JimengConfig::default());
        let credential = test_credential();

        let req_video = UnifiedRequest {
            capability: CapabilityKind::TextToVideo,
            model: "default".into(),
            prompt: "test".into(),
            negative_prompt: None,
            reference_image_path: None,
            reference_image_url: None,
            parameters: Default::default(),
        };
        assert_eq!(
            connector.effective_model(&req_video, &credential),
            "seedance-2-0"
        );

        let req_image = UnifiedRequest {
            capability: CapabilityKind::TextToImage,
            ..req_video.clone()
        };
        assert_eq!(
            connector.effective_model(&req_image, &credential),
            "jimeng-5.0"
        );

        let req_custom = UnifiedRequest {
            model: "jimeng-5.0".into(),
            ..req_video.clone()
        };
        assert_eq!(
            connector.effective_model(&req_custom, &credential),
            "jimeng-5.0"
        );
    }

    #[test]
    fn jimeng_login_url() {
        let connector = JimengConnector::new(JimengConfig::default());
        assert!(connector.login_url().contains("jimeng.jianying.com"));
    }
}
