//! 轻量语义响应缓存。
//!
//! 仅针对"直接回答"轮次（打招呼/闲聊/简单问答）：此类轮次结束后把
//! 规范化用户消息 → LLM 回答写入缓存；相同消息再次出现时直接命中缓存返回，
//! 跳过凭据加载与 LLM 调用（单次调用的成本也省掉）。
//!
//! 设计约束：
//! - 键 = 规范化用户消息（修剪/小写/空白折叠/去尾部标点）+ 会话系统提示词哈希，
//!   不同人设的会话不共享回答；
//! - 仅缓存短消息（≤ 64 字符），长请求重复率低、缓存价值低；
//! - 容量上限 128 条，超限淘汰最旧条目；
//! - 纯内存存储、生命周期随应用进程，不做持久化（刻意保持轻量）。

use std::collections::HashMap;
use std::sync::Mutex;

/// 缓存条目上限，超出时淘汰最旧条目。
const MAX_ENTRIES: usize = 128;

/// 可缓存用户消息的最大字符数。
const MAX_KEY_CHARS: usize = 64;

/// 规范化时剥离的尾部标点（重复的招呼常只差一个句尾标点）。
const TRAILING_PUNCT: &[char] = &['!', '！', '?', '？', '。', '.', '~', '～', ',', '，', '…'];

/// 单条缓存。
struct CacheEntry {
    content: String,
    inserted_at: i64,
}

/// 轻量响应缓存（内部可变性，通过 &self 共享使用）。
pub struct ResponseCache {
    entries: Mutex<HashMap<String, CacheEntry>>,
}

impl Default for ResponseCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ResponseCache {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// 规范化用户消息生成缓存键；不适合缓存（空白/过长）时返回 None。
    fn cache_key(user_content: &str, system_prompt: Option<&str>) -> Option<String> {
        // 修剪 → 小写 → 移除全部空白（中英文混排时空白分隔无键意义）。
        let mut normalized = String::with_capacity(user_content.len());
        for ch in user_content.trim().to_lowercase().chars() {
            if !ch.is_whitespace() {
                normalized.push(ch);
            }
        }
        // 去掉尾部标点，让 "你好" / "你好！" / "你好～" 命中同一条目。
        let normalized = normalized
            .trim_end_matches(|c| TRAILING_PUNCT.contains(&c))
            .trim();
        if normalized.is_empty() || normalized.chars().count() > MAX_KEY_CHARS {
            return None;
        }

        // 用系统提示词哈希区分人设，避免不同人设会话串答案。
        let persona = match system_prompt {
            Some(p) if !p.trim().is_empty() => {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                p.hash(&mut hasher);
                format!("{:x}", hasher.finish())
            }
            _ => "default".to_owned(),
        };

        Some(format!("{persona}|{normalized}"))
    }

    /// 查询缓存，命中返回缓存回答。
    pub fn get(&self, user_content: &str, system_prompt: Option<&str>) -> Option<String> {
        let key = Self::cache_key(user_content, system_prompt)?;
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.get(&key).map(|e| e.content.clone())
    }

    /// 写入缓存；仅直接回答轮次调用。空回答与不可规范化的消息不缓存。
    pub fn put(&self, user_content: &str, system_prompt: Option<&str>, response: &str) {
        if response.trim().is_empty() {
            return;
        }
        let Some(key) = Self::cache_key(user_content, system_prompt) else {
            return;
        };
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        if !entries.contains_key(&key) && entries.len() >= MAX_ENTRIES {
            // 淘汰最旧条目。
            if let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, e)| e.inserted_at)
                .map(|(k, _)| k.clone())
            {
                entries.remove(&oldest);
            }
        }
        entries.insert(
            key,
            CacheEntry {
                content: response.to_owned(),
                inserted_at: now,
            },
        );
    }

    /// 当前缓存条目数（测试/观测用）。
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_after_put_with_normalization() {
        let cache = ResponseCache::new();
        cache.put("  你好！ ", None, "你好呀，有什么可以帮你？");
        assert_eq!(
            cache.get("你好", None),
            Some("你好呀，有什么可以帮你？".to_owned())
        );
        assert_eq!(
            cache.get("你 好 ～", None),
            Some("你好呀，有什么可以帮你？".to_owned())
        );
        assert_eq!(cache.get("你好", None).is_some(), true);
    }

    #[test]
    fn case_and_whitespace_insensitive() {
        let cache = ResponseCache::new();
        cache.put("What is AIGC?", None, "AIGC 指 AI 生成内容。");
        assert!(cache.get("what   is aigc", None).is_some());
    }

    #[test]
    fn persona_isolation() {
        let cache = ResponseCache::new();
        cache.put("你好", None, "默认人设的回答");
        cache.put("你好", Some("你是猫咪专家"), "喵～");
        assert_eq!(cache.get("你好", None), Some("默认人设的回答".to_owned()));
        assert_eq!(
            cache.get("你好", Some("你是猫咪专家")),
            Some("喵～".to_owned())
        );
        assert_eq!(cache.get("你好", Some("另一个 人设")).is_none(), true);
    }

    #[test]
    fn long_message_not_cached() {
        let cache = ResponseCache::new();
        let long = "长".repeat(100);
        cache.put(&long, None, "不应被缓存");
        assert!(cache.get(&long, None).is_none());
    }

    #[test]
    fn empty_response_not_cached() {
        let cache = ResponseCache::new();
        cache.put("你好", None, "   ");
        assert!(cache.get("你好", None).is_none());
    }

    #[test]
    fn eviction_keeps_capacity() {
        let cache = ResponseCache::new();
        for i in 0..(MAX_ENTRIES + 10) {
            cache.put(&format!("问题{i}"), None, &format!("回答{i}"));
        }
        assert!(cache.len() <= MAX_ENTRIES);
    }
}
