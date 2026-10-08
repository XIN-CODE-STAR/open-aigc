//! 即梦非会员视频生成能力判断逻辑（纯函数，可单测）。
//!
//! 依据 `docs/handoff/jimeng-video-capability-logic.md` 的五步决策：
//! Step1 能力闸门 → Step2 模型解析 → Step3 成本计算 → Step4 积分校验 → Step5 错误码映射。
//!
//! 设计要点：
//! - **能力清单是权威来源**（由即梦按账户下发）；注册时间仅用于解释与兜底。
//! - 成本优先取即梦下发的预估成本；本地费率表仅作离线回退（费率待实测校准）。
//! - 新账号特例：可用模型可能仅 `seedance-2.0-mini` + 3 次试用。

use serde::{Deserialize, Serialize};

/// 会员等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Membership {
    /// 非会员。
    #[default]
    None,
    /// 会员。
    Vip,
    /// 超级会员。
    SuperVip,
}

impl Membership {
    /// 是否解锁 VIP 专属模型。
    pub fn unlocks_vip_models(self) -> bool {
        matches!(self, Self::Vip | Self::SuperVip)
    }
}

/// 账户可用的视频模型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoModel {
    /// 模型标识（如 `seedance-2.0-mini`）。
    pub id: String,
    /// VIP 专属（非会员锁定）。
    pub vip_locked: bool,
}

/// 试用 / 每日免费额度。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeQuota {
    /// 适用模型。
    pub model: String,
    /// 剩余次数。
    pub remaining: u32,
}

/// 账户视频能力画像。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountVideoProfile {
    pub membership: Membership,
    /// 账户年龄（天）。**仅用于解释与兜底**，不作为权威判据。
    pub account_age_days: u32,
    /// 剩余积分（`/token/points` 的 totalCredit）。
    pub credits_total: i64,
    /// 即梦下发的可用视频模型清单。
    pub video_models: Vec<VideoModel>,
    /// 新账号试用（如 `seedance-2.0-mini` × 3）。
    pub free_trials: Option<FreeQuota>,
    /// 每日免费额度（如"每天 1 次"）。
    pub daily_free: Option<FreeQuota>,
}

/// 新账号判定阈值（天）。仅用于文案与兜底推断。
pub const NEW_ACCOUNT_DAYS: u32 = 7;

impl AccountVideoProfile {
    /// 是否为新账号（注册时间维度）。
    pub fn is_new_account(&self) -> bool {
        self.account_age_days < NEW_ACCOUNT_DAYS
    }
}

/// 视频生成请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoRequest {
    /// 请求的模型标识。
    pub model: String,
    /// 分辨率（如 720p / 1080p）。
    pub resolution: String,
    /// 时长（秒）。
    pub duration_secs: u32,
}

/// 判断结果。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VideoDecision {
    /// 放行。`cost` 为实际扣减积分（试用/每日免费时为 0）。
    Proceed {
        requested_model: String,
        used_model: String,
        cost: i64,
        uses_trial: bool,
        uses_daily_free: bool,
    },
    /// 该账户不支持视频生成。
    Unsupported { reason: String },
    /// 积分不足。
    InsufficientCredits {
        model: String,
        needed: i64,
        available: i64,
        shortfall: i64,
    },
}

/// 该账户可用的视频模型（VIP 会员解锁全部；非会员过滤 `vip_locked`）。
pub fn usable_models(profile: &AccountVideoProfile) -> Vec<&VideoModel> {
    profile
        .video_models
        .iter()
        .filter(|m| profile.membership.unlocks_vip_models() || !m.vip_locked)
        .collect()
}

/// 每秒基础积分费率（**离线回退表**，待实测校准）。
///
/// 依据截图：15s / 720P = 75 积分 → 720p 基准 5 积分/秒。
fn per_second_rate(model: &str, resolution: &str) -> f64 {
    let lower = model.to_lowercase();
    let base = if lower.contains("mini") {
        3.0
    } else if lower.contains("fast") {
        4.0
    } else {
        5.0
    };
    let factor = match normalize_resolution(resolution) {
        "480p" => 0.5,
        "1080p" => 1.8,
        _ => 1.0, // 720p 基准
    };
    base * factor
}

/// 归一化分辨率字符串（大小写、`P`/`p`）。
fn normalize_resolution(resolution: &str) -> &'static str {
    match resolution.trim().to_lowercase().as_str() {
        "480p" | "480" => "480p",
        "1080p" | "1080" => "1080p",
        _ => "720p",
    }
}

/// 计算成本：`ceil(时长 × 费率(模型, 分辨率))`。
///
/// 实际优先使用即梦下发的预估成本；本函数仅作离线回退。
pub fn compute_cost(model: &str, resolution: &str, duration_secs: u32) -> i64 {
    (per_second_rate(model, resolution) * f64::from(duration_secs)).ceil() as i64
}

/// 五步决策主入口。
pub fn evaluate(profile: &AccountVideoProfile, request: &VideoRequest) -> VideoDecision {
    // Step 1 · 能力闸门
    let usable = usable_models(profile);
    if usable.is_empty() {
        let reason = if profile.is_new_account() {
            "新账号暂无可用视频模型（试用已用尽）".to_owned()
        } else {
            "当前账号暂不支持视频生成".to_owned()
        };
        return VideoDecision::Unsupported { reason };
    }

    // Step 2 · 模型解析（不可用 / VIP 锁定 → 降级到可用清单首个）
    let used_model = if usable.iter().any(|m| m.id == request.model) {
        request.model.clone()
    } else {
        usable
            .first()
            .map(|m| m.id.clone())
            .unwrap_or_else(|| request.model.clone())
    };

    // Step 3 · 成本计算（试用 / 每日免费命中 → 0）
    let trial_hit = profile
        .free_trials
        .as_ref()
        .is_some_and(|q| q.remaining > 0 && q.model == used_model);
    let daily_hit = profile
        .daily_free
        .as_ref()
        .is_some_and(|q| q.remaining > 0 && q.model == used_model);

    let (cost, uses_trial, uses_daily_free) = if trial_hit {
        (0, true, false)
    } else if daily_hit {
        (0, false, true)
    } else {
        (
            compute_cost(&used_model, &request.resolution, request.duration_secs),
            false,
            false,
        )
    };

    // Step 4 · 积分校验
    if cost > 0 && profile.credits_total < cost {
        return VideoDecision::InsufficientCredits {
            model: used_model,
            needed: cost,
            available: profile.credits_total,
            shortfall: cost - profile.credits_total,
        };
    }

    VideoDecision::Proceed {
        requested_model: request.model.clone(),
        used_model,
        cost,
        uses_trial,
        uses_daily_free,
    }
}

/// 后端错误归类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VideoFailure {
    /// 积分不足（-2009 / 1006）→ 触发积分弹窗。
    InsufficientCredits { code: i64 },
    /// 高峰期容量限制（1310）→ 稍后重试。
    RateLimited { code: i64 },
    /// 模型已下线（2061）→ 刷新模型清单。
    ModelRetired { code: i64 },
    /// 风控拦截（4013）→ 换通道 / 模型。
    RiskControlled { code: i64 },
    /// 登录态失效（1015）→ 重新登录。
    SessionInvalid { code: i64 },
    /// 未知错误。
    Unknown { code: i64 },
}

/// Step 5 · 后端错误码 → 归类。
pub fn map_backend_error(code: i64) -> VideoFailure {
    match code {
        -2009 | 1006 => VideoFailure::InsufficientCredits { code },
        1310 => VideoFailure::RateLimited { code },
        2061 => VideoFailure::ModelRetired { code },
        4013 => VideoFailure::RiskControlled { code },
        1015 => VideoFailure::SessionInvalid { code },
        _ => VideoFailure::Unknown { code },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str, vip_locked: bool) -> VideoModel {
        VideoModel {
            id: id.to_owned(),
            vip_locked,
        }
    }

    fn request(model: &str, resolution: &str, duration: u32) -> VideoRequest {
        VideoRequest {
            model: model.to_owned(),
            resolution: resolution.to_owned(),
            duration_secs: duration,
        }
    }

    /// 老账户：非会员，有标准档模型，余额充足。
    fn old_account(credits: i64) -> AccountVideoProfile {
        AccountVideoProfile {
            membership: Membership::None,
            account_age_days: 120,
            credits_total: credits,
            video_models: vec![
                model("seedance-2.0-fast", false),
                model("seedance-2.0", false),
            ],
            free_trials: None,
            daily_free: None,
        }
    }

    /// 新账户：仅 mini 可用 + 3 次试用。
    fn new_account(credits: i64, trial_remaining: u32) -> AccountVideoProfile {
        AccountVideoProfile {
            membership: Membership::None,
            account_age_days: 1,
            credits_total: credits,
            video_models: vec![
                model("seedance-2.0-mini", false),
                model("seedance-2.0-vip", true),
            ],
            free_trials: Some(FreeQuota {
                model: "seedance-2.0-mini".to_owned(),
                remaining: trial_remaining,
            }),
            daily_free: None,
        }
    }

    #[test]
    fn old_account_sufficient_credits_proceeds() {
        let decision = evaluate(
            &old_account(200),
            &request("seedance-2.0-fast", "720p", 15),
        );
        match decision {
            VideoDecision::Proceed {
                used_model,
                cost,
                uses_trial,
                uses_daily_free,
                ..
            } => {
                assert_eq!(used_model, "seedance-2.0-fast");
                // fast 档 4/秒 × 15 = 60
                assert_eq!(cost, 60);
                assert!(!uses_trial && !uses_daily_free);
            }
            other => panic!("expected Proceed, got {other:?}"),
        }
    }

    #[test]
    fn base_rate_matches_screenshot() {
        // 截图：15s / 720P = 75 积分 → 标准档 5/秒
        assert_eq!(compute_cost("seedance-2.0", "720p", 15), 75);
    }

    #[test]
    fn old_account_insufficient_credits() {
        let decision = evaluate(&old_account(60), &request("seedance-2.0", "720p", 15));
        match decision {
            VideoDecision::InsufficientCredits {
                needed,
                available,
                shortfall,
                ..
            } => {
                assert_eq!(needed, 75);
                assert_eq!(available, 60);
                assert_eq!(shortfall, 15);
            }
            other => panic!("expected InsufficientCredits, got {other:?}"),
        }
    }

    #[test]
    fn new_account_trial_costs_zero() {
        let decision = evaluate(&new_account(0, 3), &request("seedance-2.0-mini", "720p", 5));
        match decision {
            VideoDecision::Proceed {
                cost,
                uses_trial,
                used_model,
                ..
            } => {
                assert_eq!(used_model, "seedance-2.0-mini");
                assert_eq!(cost, 0);
                assert!(uses_trial);
            }
            other => panic!("expected Proceed(trial), got {other:?}"),
        }
    }

    #[test]
    fn new_account_trial_exhausted_unsupported() {
        // 试用耗尽 + 唯一非 VIP 模型是 mini（remaining=0）→ 仍可用 mini 但需积分；
        // 若账户无积分则报积分不足。这里构造"无任何可用非 VIP 模型"的场景。
        let mut profile = new_account(0, 0);
        profile.video_models = vec![model("seedance-2.0-vip", true)];
        let decision = evaluate(&profile, &request("seedance-2.0-vip", "720p", 5));
        assert!(matches!(decision, VideoDecision::Unsupported { .. }));
    }

    #[test]
    fn new_account_no_credits_no_trial_insufficient() {
        // 试用耗尽、mini 仍可用但无积分 → 积分不足
        let decision = evaluate(&new_account(0, 0), &request("seedance-2.0-mini", "720p", 5));
        assert!(matches!(
            decision,
            VideoDecision::InsufficientCredits { .. }
        ));
    }

    #[test]
    fn non_member_vip_model_downgrades() {
        // 非会员请求 VIP 模型 → 降级到可用档
        let decision = evaluate(
            &new_account(100, 0),
            &request("seedance-2.0-vip", "720p", 5),
        );
        match decision {
            VideoDecision::Proceed {
                requested_model,
                used_model,
                ..
            } => {
                assert_eq!(requested_model, "seedance-2.0-vip");
                assert_eq!(used_model, "seedance-2.0-mini");
            }
            other => panic!("expected Proceed(downgrade), got {other:?}"),
        }
    }

    #[test]
    fn vip_member_uses_vip_model() {
        let mut profile = new_account(100, 0);
        profile.membership = Membership::Vip;
        let decision = evaluate(&profile, &request("seedance-2.0-vip", "720p", 5));
        match decision {
            VideoDecision::Proceed { used_model, .. } => {
                assert_eq!(used_model, "seedance-2.0-vip");
            }
            other => panic!("expected Proceed(vip), got {other:?}"),
        }
    }

    #[test]
    fn daily_free_hit_costs_zero() {
        let mut profile = old_account(0);
        profile.daily_free = Some(FreeQuota {
            model: "seedance-2.0-fast".to_owned(),
            remaining: 1,
        });
        let decision = evaluate(&profile, &request("seedance-2.0-fast", "720p", 10));
        match decision {
            VideoDecision::Proceed {
                cost,
                uses_daily_free,
                ..
            } => {
                assert_eq!(cost, 0);
                assert!(uses_daily_free);
            }
            other => panic!("expected Proceed(daily free), got {other:?}"),
        }
    }

    #[test]
    fn maps_backend_error_codes() {
        assert!(matches!(
            map_backend_error(-2009),
            VideoFailure::InsufficientCredits { .. }
        ));
        assert!(matches!(
            map_backend_error(1006),
            VideoFailure::InsufficientCredits { .. }
        ));
        assert!(matches!(map_backend_error(1310), VideoFailure::RateLimited { .. }));
        assert!(matches!(
            map_backend_error(2061),
            VideoFailure::ModelRetired { .. }
        ));
        assert!(matches!(
            map_backend_error(4013),
            VideoFailure::RiskControlled { .. }
        ));
        assert!(matches!(
            map_backend_error(1015),
            VideoFailure::SessionInvalid { .. }
        ));
        assert!(matches!(map_backend_error(42), VideoFailure::Unknown { .. }));
    }
}
