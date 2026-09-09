use std::collections::HashMap;
use std::sync::Mutex;

use crate::domain::semantic::ArtifactSemanticProfile;

/// Cache key for analysis results.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct AnalysisCacheKey {
    /// Asset ID
    pub asset_id: String,
    /// Analysis type (e.g., "caption", "ocr", "vision")
    pub analysis_type: String,
    /// Model used
    pub model: String,
    /// Model version
    pub model_version: String,
    /// Prompt version
    pub prompt_version: String,
}

impl AnalysisCacheKey {
    pub fn new(
        asset_id: String,
        analysis_type: String,
        model: String,
        model_version: String,
        prompt_version: String,
    ) -> Self {
        Self {
            asset_id,
            analysis_type,
            model,
            model_version,
            prompt_version,
        }
    }

    /// Create a cache key from a profile.
    pub fn from_profile(profile: &ArtifactSemanticProfile) -> Self {
        Self {
            asset_id: profile.artifact_id.clone(),
            analysis_type: "caption".to_owned(), // Default
            model: profile.analyzer.clone(),
            model_version: profile.analyzer_version.clone(),
            prompt_version: "1.0".to_owned(), // Default
        }
    }
}

/// Analysis cache entry.
#[derive(Debug, Clone)]
pub struct AnalysisCacheEntry {
    /// The cached profile
    pub profile: ArtifactSemanticProfile,
    /// When the entry was cached
    pub cached_at: String,
    /// Cache hit count
    pub hit_count: u32,
}

/// Analysis cache service.
///
/// Caches analysis results to avoid redundant processing.
/// Uses a simple in-memory cache with LRU-like eviction.
pub struct AnalysisCacheService {
    cache: Mutex<HashMap<AnalysisCacheKey, AnalysisCacheEntry>>,
    max_size: usize,
}

impl AnalysisCacheService {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            max_size,
        }
    }

    /// Get a cached analysis result.
    pub fn get(&self, key: &AnalysisCacheKey) -> Option<ArtifactSemanticProfile> {
        let mut cache = self.cache.lock().ok()?;
        if let Some(entry) = cache.get_mut(key) {
            entry.hit_count += 1;
            Some(entry.profile.clone())
        } else {
            None
        }
    }

    /// Store an analysis result in the cache.
    pub fn put(&self, key: AnalysisCacheKey, profile: ArtifactSemanticProfile) {
        let mut cache = match self.cache.lock() {
            Ok(cache) => cache,
            Err(_) => return,
        };

        // If cache is full, remove the least recently used entry
        if cache.len() >= self.max_size {
            // Find the entry with the lowest hit count
            if let Some(lru_key) = cache
                .iter()
                .min_by_key(|(_, entry)| entry.hit_count)
                .map(|(key, _)| key.clone())
            {
                cache.remove(&lru_key);
            }
        }

        let entry = AnalysisCacheEntry {
            profile,
            cached_at: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned()),
            hit_count: 0,
        };

        cache.insert(key, entry);
    }

    /// Check if a key is in the cache.
    pub fn contains(&self, key: &AnalysisCacheKey) -> bool {
        let cache = match self.cache.lock() {
            Ok(cache) => cache,
            Err(_) => return false,
        };
        cache.contains_key(key)
    }

    /// Remove an entry from the cache.
    pub fn remove(&self, key: &AnalysisCacheKey) -> Option<ArtifactSemanticProfile> {
        let mut cache = self.cache.lock().ok()?;
        cache.remove(key).map(|entry| entry.profile)
    }

    /// Clear the cache.
    pub fn clear(&self) {
        let mut cache = match self.cache.lock() {
            Ok(cache) => cache,
            Err(_) => return,
        };
        cache.clear();
    }

    /// Get the current cache size.
    pub fn size(&self) -> usize {
        let cache = match self.cache.lock() {
            Ok(cache) => cache,
            Err(_) => return 0,
        };
        cache.len()
    }

    /// Get cache statistics.
    pub fn stats(&self) -> CacheStats {
        let cache = match self.cache.lock() {
            Ok(cache) => cache,
            Err(_) => {
                return CacheStats {
                    size: 0,
                    max_size: self.max_size,
                    total_hits: 0,
                    avg_hits: 0.0,
                }
            }
        };

        let total_hits: u32 = cache.values().map(|entry| entry.hit_count).sum();
        let avg_hits = if cache.is_empty() {
            0.0
        } else {
            total_hits as f32 / cache.len() as f32
        };

        CacheStats {
            size: cache.len(),
            max_size: self.max_size,
            total_hits,
            avg_hits,
        }
    }
}

/// Cache statistics.
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub size: usize,
    pub max_size: usize,
    pub total_hits: u32,
    pub avg_hits: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::common::InferenceSource;
    use crate::domain::semantic::SemanticTag;

    fn create_test_profile(asset_id: &str) -> ArtifactSemanticProfile {
        ArtifactSemanticProfile {
            artifact_id: asset_id.to_owned(),
            caption: Some("Test caption".to_owned()),
            ocr_text: None,
            tags: vec![SemanticTag {
                name: "test".to_owned(),
                confidence: 0.9,
                source: InferenceSource::Llm,
            }],
            entities: vec![],
            embedding_id: None,
            analyzer: "test".to_owned(),
            analyzer_version: "1.0".to_owned(),
            analyzed_at: "2026-08-17T00:00:00Z".to_owned(),
            description_short: None,
            description_detailed: None,
            objects: vec![],
            scene: vec![],
            actions: vec![],
            concepts: vec![],
            relations: vec![],
            analysis_job_id: None,
        }
    }

    #[test]
    fn cache_stores_and_retrieves() {
        let cache = AnalysisCacheService::new(10);
        let key = AnalysisCacheKey::new(
            "asset-1".to_owned(),
            "caption".to_owned(),
            "test".to_owned(),
            "1.0".to_owned(),
            "1.0".to_owned(),
        );
        let profile = create_test_profile("asset-1");

        cache.put(key.clone(), profile.clone());
        let retrieved = cache.get(&key).unwrap();
        assert_eq!(retrieved.artifact_id, "asset-1");
    }

    #[test]
    fn cache_evicts_lru() {
        let cache = AnalysisCacheService::new(2);

        let key1 = AnalysisCacheKey::new(
            "asset-1".to_owned(),
            "caption".to_owned(),
            "test".to_owned(),
            "1.0".to_owned(),
            "1.0".to_owned(),
        );
        let key2 = AnalysisCacheKey::new(
            "asset-2".to_owned(),
            "caption".to_owned(),
            "test".to_owned(),
            "1.0".to_owned(),
            "1.0".to_owned(),
        );
        let key3 = AnalysisCacheKey::new(
            "asset-3".to_owned(),
            "caption".to_owned(),
            "test".to_owned(),
            "1.0".to_owned(),
            "1.0".to_owned(),
        );

        cache.put(key1.clone(), create_test_profile("asset-1"));
        cache.put(key2.clone(), create_test_profile("asset-2"));

        // Access key1 to increase its hit count
        cache.get(&key1);

        // Add key3, should evict key2 (lower hit count)
        cache.put(key3.clone(), create_test_profile("asset-3"));

        assert!(cache.contains(&key1));
        assert!(!cache.contains(&key2));
        assert!(cache.contains(&key3));
    }
}
