//! 三通道健康检查冒烟测试（默认忽略，手工触发）。
//!
//! 触发方式：`node scripts/rust-test.mjs -- --ignored channel_health --nocapture`
//! （通常由 scripts/channel-health.mjs 编排调用）。
//!
//! 凭据只从环境变量读取（技术决策 D8：源码/测试不写入凭据字面量）：
//! - KLING_ACCESS_KEY / KLING_SECRET_KEY：可灵 AK/SK
//! - SEEDANCE_API_KEY：火山方舟 API Key
//! - 提交真实生成任务（计费真实发生）需额外设置 AIGC_CHANNEL_SUBMIT=1，
//!   由 channel-health.mjs 的 --submit 注入；默认只跑免费的 health_check。
//!
//! 缺失对应环境变量时自动跳过，不视为失败。
//!
//! 即梦通道不走本文件：会话凭据在应用内管理，脚本直接探测本地代理
//! （127.0.0.1:5100，jimeng_connector.rs 同款端点）。

use crate::{
    adapters::providers::{
        kling_video::{KlingConfig, KlingVideoAdapter},
        seedance_video::{SeedanceConfig, SeedanceVideoAdapter},
    },
    domain::credentials::{CredentialContext, CredentialType},
    ports::unified_provider::{CapabilityKind, UnifiedProviderAdapter, UnifiedRequest},
};

/// 是否允许提交真实计费任务（channel-health.mjs --submit 注入）。
fn submit_enabled() -> bool {
    std::env::var("AIGC_CHANNEL_SUBMIT").ok().as_deref() == Some("1")
}

/// 从环境变量构造可灵凭据；缺失时返回 None（调用方跳过测试）。
fn kling_credential() -> Option<CredentialContext> {
    let access_key = std::env::var("KLING_ACCESS_KEY").ok()?;
    let secret_key = std::env::var("KLING_SECRET_KEY").ok()?;
    Some(CredentialContext {
        provider_id: "kling".to_owned(),
        credential_type: CredentialType::AccessSecret,
        payload: serde_json::json!({ "access_key": access_key, "secret_key": secret_key }),
        base_url: String::new(),
        model: String::new(),
    })
}

/// 从环境变量构造 Seedance（火山方舟）凭据；缺失时返回 None。
fn seedance_credential() -> Option<CredentialContext> {
    let api_key = std::env::var("SEEDANCE_API_KEY").ok()?;
    Some(CredentialContext {
        provider_id: "seedance".to_owned(),
        credential_type: CredentialType::ApiKey,
        payload: serde_json::json!({ "api_key": api_key }),
        base_url: String::new(),
        model: String::new(),
    })
}

#[test]
#[ignore]
fn kling_health_smoke() {
    let credential = match kling_credential() {
        Some(c) => c,
        None => {
            eprintln!("skip: 未设置 KLING_ACCESS_KEY / KLING_SECRET_KEY");
            return;
        }
    };
    let adapter = KlingVideoAdapter::new(KlingConfig::default());
    let health = adapter
        .health_check(&credential)
        .expect("health_check 不应返回 Err（不可用通过 available=false 表达）");
    assert!(health.available, "Kling 健康检查失败: {}", health.message);
    println!("[kling] health OK: {}", health.message);
}

#[test]
#[ignore]
fn kling_submit_minimal_video_smoke() {
    if !submit_enabled() {
        eprintln!("skip: 未设置 AIGC_CHANNEL_SUBMIT=1（提交会产生真实计费）");
        return;
    }
    let credential = match kling_credential() {
        Some(c) => c,
        None => {
            eprintln!("skip: 未设置 KLING_ACCESS_KEY / KLING_SECRET_KEY");
            return;
        }
    };
    let adapter = KlingVideoAdapter::new(KlingConfig::default());
    let mut parameters = std::collections::HashMap::new();
    // 厂商最低档：5s / std 模式，把计费压到最低
    parameters.insert("duration".to_owned(), serde_json::json!("5"));
    parameters.insert("mode".to_owned(), serde_json::json!("std"));
    let request = UnifiedRequest {
        capability: CapabilityKind::TextToVideo,
        model: "default".to_owned(),
        prompt: "a calm ocean wave, health check test clip".to_owned(),
        negative_prompt: None,
        reference_image_path: None,
        reference_image_url: None,
        parameters,
    };
    let submitted = adapter
        .submit(&request, &credential)
        .expect("kling 提交失败");
    println!("[kling] submitted: {}", submitted.remote_job_id);
    // 只做一次轮询确认任务可查询，不等待生成完成（轮询免费，生成已计费）
    let poll = adapter
        .poll(&submitted.remote_job_id, &credential)
        .expect("kling 轮询失败");
    println!("[kling] poll status: {} ({}%)", poll.status, poll.progress);
}

#[test]
#[ignore]
fn seedance_health_smoke() {
    let credential = match seedance_credential() {
        Some(c) => c,
        None => {
            eprintln!("skip: 未设置 SEEDANCE_API_KEY");
            return;
        }
    };
    let adapter = SeedanceVideoAdapter::new(SeedanceConfig::default());
    let health = adapter
        .health_check(&credential)
        .expect("health_check 不应返回 Err（不可用通过 available=false 表达）");
    assert!(
        health.available,
        "Seedance 健康检查失败: {}",
        health.message
    );
    println!("[seedance] health OK: {}", health.message);
}

#[test]
#[ignore]
fn seedance_submit_minimal_video_smoke() {
    if !submit_enabled() {
        eprintln!("skip: 未设置 AIGC_CHANNEL_SUBMIT=1（提交会产生真实计费）");
        return;
    }
    let credential = match seedance_credential() {
        Some(c) => c,
        None => {
            eprintln!("skip: 未设置 SEEDANCE_API_KEY");
            return;
        }
    };
    let adapter = SeedanceVideoAdapter::new(SeedanceConfig::default());
    let mut parameters = std::collections::HashMap::new();
    // 厂商最低档：5s / 480p / 关闭音频生成
    parameters.insert("duration".to_owned(), serde_json::json!(5));
    parameters.insert("resolution".to_owned(), serde_json::json!("480p"));
    parameters.insert("ratio".to_owned(), serde_json::json!("16:9"));
    parameters.insert("generate_audio".to_owned(), serde_json::json!(false));
    let request = UnifiedRequest {
        capability: CapabilityKind::TextToVideo,
        model: "default".to_owned(),
        prompt: "a calm ocean wave, health check test clip".to_owned(),
        negative_prompt: None,
        reference_image_path: None,
        reference_image_url: None,
        parameters,
    };
    let submitted = adapter
        .submit(&request, &credential)
        .expect("seedance 提交失败");
    println!("[seedance] submitted: {}", submitted.remote_job_id);
    let poll = adapter
        .poll(&submitted.remote_job_id, &credential)
        .expect("seedance 轮询失败");
    println!(
        "[seedance] poll status: {} ({}%)",
        poll.status, poll.progress
    );
}
