//! 多账号调度器。
//!
//! 当同一 provider 存在多个 resource_account 时，AccountScheduler 负责：
//! 1. 维护每个 provider 的账号池
//! 2. 根据策略（优先级 / 轮询 / 故障转移）选择最优账号
//! 3. 记录失败次数，自动降级不可用账号
//!
//! 集成方式：generation_engine 注册账号时调用 `add_account()`，
//! 提交生成请求时调用 `select()` 获取应使用的 credential_key。

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;

/// 调度策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScheduleStrategy {
    /// 按优先级排序，始终选最高优先级的可用账号。
    #[default]
    Priority,
    /// 在可用账号间轮询。
    RoundRobin,
    /// 优先使用最近成功的账号，失败时切换到下一个。
    Failover,
}

/// 账号池中的单个条目。
#[derive(Debug, Clone)]
pub struct AccountEntry {
    pub account_id: String,
    pub credential_key: String,
    /// 优先级（数值越小越优先）。
    pub priority: u32,
    /// 连续失败次数。
    pub consecutive_failures: u32,
    /// 是否被标记为不可用（手动禁用或连续失败超限）。
    pub degraded: bool,
}

impl AccountEntry {
    /// 是否可参与调度。
    pub fn is_available(&self) -> bool {
        !self.degraded && self.consecutive_failures < MAX_CONSECUTIVE_FAILURES
    }
}

/// 连续失败阈值：超过此值自动降级。
const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// 多账号调度器。
pub struct AccountScheduler {
    /// provider_id → 账号池
    pools: RwLock<HashMap<String, Vec<AccountEntry>>>,
    /// provider_id → 轮询计数器
    round_robin_counters: RwLock<HashMap<String, AtomicUsize>>,
    /// 全局调度策略（后续可扩展为 per-provider）。
    strategy: ScheduleStrategy,
}

impl AccountScheduler {
    pub fn new(strategy: ScheduleStrategy) -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
            round_robin_counters: RwLock::new(HashMap::new()),
            strategy,
        }
    }

    /// 添加账号到指定 provider 的池中。
    pub fn add_account(&self, provider_id: &str, entry: AccountEntry) {
        let mut pools = self.pools.write().unwrap_or_else(|e| e.into_inner());
        let pool = pools.entry(provider_id.to_owned()).or_default();

        // 避免重复注册（按 account_id 去重）
        if pool.iter().any(|e| e.account_id == entry.account_id) {
            return;
        }
        pool.push(entry);

        // 按优先级排序
        pool.sort_by_key(|e| e.priority);

        // 初始化轮询计数器
        let mut counters = self
            .round_robin_counters
            .write()
            .unwrap_or_else(|e| e.into_inner());
        counters
            .entry(provider_id.to_owned())
            .or_insert_with(|| AtomicUsize::new(0));
    }

    /// 移除账号（删除/禁用时调用）。
    pub fn remove_account(&self, provider_id: &str, account_id: &str) {
        let mut pools = self.pools.write().unwrap_or_else(|e| e.into_inner());
        if let Some(pool) = pools.get_mut(provider_id) {
            pool.retain(|e| e.account_id != account_id);
        }
    }

    /// 选择下一个应使用的账号。返回 credential_key。
    ///
    /// 如果该 provider 只有一个账号，直接返回。
    /// 如果有多个，按策略选择。
    /// 如果没有可用账号，返回 None。
    pub fn select(&self, provider_id: &str) -> Option<String> {
        let pools = self.pools.read().unwrap_or_else(|e| e.into_inner());
        let pool = pools.get(provider_id)?;

        let available: Vec<&AccountEntry> = pool.iter().filter(|e| e.is_available()).collect();
        if available.is_empty() {
            return None;
        }

        let selected = match self.strategy {
            ScheduleStrategy::Priority => {
                // 已按 priority 排序，取第一个可用的
                available[0]
            }
            ScheduleStrategy::RoundRobin => {
                let counters = self
                    .round_robin_counters
                    .read()
                    .unwrap_or_else(|e| e.into_inner());
                let counter = counters.get(provider_id)?;
                let idx = counter.fetch_add(1, Ordering::Relaxed) % available.len();
                available[idx]
            }
            ScheduleStrategy::Failover => {
                // 选连续失败最少的（最"新鲜"的）
                available
                    .iter()
                    .min_by_key(|e| e.consecutive_failures)
                    .copied()
                    .unwrap_or(available[0])
            }
        };

        Some(selected.credential_key.clone())
    }

    /// 报告成功：重置连续失败计数。
    pub fn report_success(&self, provider_id: &str, account_id: &str) {
        let mut pools = self.pools.write().unwrap_or_else(|e| e.into_inner());
        if let Some(pool) = pools.get_mut(provider_id) {
            if let Some(entry) = pool.iter_mut().find(|e| e.account_id == account_id) {
                entry.consecutive_failures = 0;
                entry.degraded = false;
            }
        }
    }

    /// 报告失败：递增连续失败计数，超限自动降级。
    pub fn report_failure(&self, provider_id: &str, account_id: &str) {
        let mut pools = self.pools.write().unwrap_or_else(|e| e.into_inner());
        if let Some(pool) = pools.get_mut(provider_id) {
            if let Some(entry) = pool.iter_mut().find(|e| e.account_id == account_id) {
                entry.consecutive_failures += 1;
                if entry.consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    entry.degraded = true;
                    eprintln!(
                        "[Scheduler] Account {account_id} degraded after {} consecutive failures",
                        entry.consecutive_failures
                    );
                }
            }
        }
    }

    /// 手动设置账号降级状态（健康检查回写时调用）。
    pub fn set_degraded(&self, provider_id: &str, account_id: &str, degraded: bool) {
        let mut pools = self.pools.write().unwrap_or_else(|e| e.into_inner());
        if let Some(pool) = pools.get_mut(provider_id) {
            if let Some(entry) = pool.iter_mut().find(|e| e.account_id == account_id) {
                entry.degraded = degraded;
                if !degraded {
                    entry.consecutive_failures = 0;
                }
            }
        }
    }

    /// 获取指定 provider 的可用账号数量。
    #[allow(dead_code)]
    pub fn available_count(&self, provider_id: &str) -> usize {
        let pools = self.pools.read().unwrap_or_else(|e| e.into_inner());
        pools
            .get(provider_id)
            .map(|pool| pool.iter().filter(|e| e.is_available()).count())
            .unwrap_or(0)
    }

    /// 列出所有已注册的 provider ID。
    #[allow(dead_code)]
    pub fn list_providers(&self) -> Vec<String> {
        let pools = self.pools.read().unwrap_or_else(|e| e.into_inner());
        pools.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(id: &str, priority: u32) -> AccountEntry {
        AccountEntry {
            account_id: id.to_owned(),
            credential_key: format!("key:{id}"),
            priority,
            consecutive_failures: 0,
            degraded: false,
        }
    }

    #[test]
    fn priority_strategy_selects_highest() {
        let scheduler = AccountScheduler::new(ScheduleStrategy::Priority);
        scheduler.add_account("jimeng", make_entry("a2", 2));
        scheduler.add_account("jimeng", make_entry("a1", 1));
        scheduler.add_account("jimeng", make_entry("a3", 3));

        assert_eq!(scheduler.select("jimeng"), Some("key:a1".to_owned()));
    }

    #[test]
    fn round_robin_cycles() {
        let scheduler = AccountScheduler::new(ScheduleStrategy::RoundRobin);
        scheduler.add_account("jimeng", make_entry("a1", 1));
        scheduler.add_account("jimeng", make_entry("a2", 2));

        let first = scheduler.select("jimeng");
        let second = scheduler.select("jimeng");
        let third = scheduler.select("jimeng");

        assert_ne!(first, second);
        assert_eq!(first, third); // 回到第一个
    }

    #[test]
    fn failover_skips_failed() {
        let scheduler = AccountScheduler::new(ScheduleStrategy::Failover);
        scheduler.add_account("jimeng", make_entry("a1", 1));
        scheduler.add_account("jimeng", make_entry("a2", 2));

        // a1 失败两次
        scheduler.report_failure("jimeng", "a1");
        scheduler.report_failure("jimeng", "a1");

        // 应选 a2（失败次数为 0）
        assert_eq!(scheduler.select("jimeng"), Some("key:a2".to_owned()));
    }

    #[test]
    fn degraded_account_excluded() {
        let scheduler = AccountScheduler::new(ScheduleStrategy::Priority);
        scheduler.add_account("jimeng", make_entry("a1", 1));
        scheduler.add_account("jimeng", make_entry("a2", 2));

        scheduler.set_degraded("jimeng", "a1", true);

        // a1 降级，应选 a2
        assert_eq!(scheduler.select("jimeng"), Some("key:a2".to_owned()));
    }

    #[test]
    fn all_degraded_returns_none() {
        let scheduler = AccountScheduler::new(ScheduleStrategy::Priority);
        scheduler.add_account("jimeng", make_entry("a1", 1));
        scheduler.set_degraded("jimeng", "a1", true);

        assert_eq!(scheduler.select("jimeng"), None);
    }

    #[test]
    fn success_resets_failures() {
        let scheduler = AccountScheduler::new(ScheduleStrategy::Failover);
        scheduler.add_account("jimeng", make_entry("a1", 1));

        scheduler.report_failure("jimeng", "a1");
        scheduler.report_failure("jimeng", "a1");
        scheduler.report_success("jimeng", "a1");

        // 重置后应可用
        assert_eq!(scheduler.select("jimeng"), Some("key:a1".to_owned()));
    }

    #[test]
    fn remove_account_works() {
        let scheduler = AccountScheduler::new(ScheduleStrategy::Priority);
        scheduler.add_account("jimeng", make_entry("a1", 1));
        scheduler.add_account("jimeng", make_entry("a2", 2));

        scheduler.remove_account("jimeng", "a1");
        assert_eq!(scheduler.select("jimeng"), Some("key:a2".to_owned()));
    }
}
