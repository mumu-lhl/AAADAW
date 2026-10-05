use crate::DecodedAudioSource;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

/// Maximum decoded PCM retained by one process-wide audio cache.
pub const DECODED_AUDIO_CACHE_BYTES: usize = 64 * 1024 * 1024;

/// Maximum number of decoded source entries retained by one cache.
pub const DECODED_AUDIO_CACHE_ENTRIES: usize = 4_096;

/// Maximum decoded PCM retained for one media source.
pub const MAX_CACHED_AUDIO_SOURCE_BYTES: usize = 8 * 1024 * 1024;

/// Identity of an immutable embedded source and the output-rate processing profile.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DecodedAudioCacheKey {
    pub source_hash: [u8; 32],
    pub output_sample_rate: u32,
}

#[derive(Default)]
struct CacheState {
    entries: HashMap<DecodedAudioCacheKey, Arc<DecodedAudioSource>>,
    least_recent_first: VecDeque<DecodedAudioCacheKey>,
    bytes: usize,
}

/// Bounded LRU for fully decoded, short embedded sources.
#[derive(Clone)]
pub struct DecodedAudioCache {
    state: Arc<Mutex<CacheState>>,
    max_bytes: usize,
    max_entry_bytes: usize,
}

impl Default for DecodedAudioCache {
    fn default() -> Self {
        Self::with_limits(DECODED_AUDIO_CACHE_BYTES, MAX_CACHED_AUDIO_SOURCE_BYTES)
    }
}

impl DecodedAudioCache {
    /// Creates a cache with explicit total and per-source limits.
    pub fn with_limits(max_bytes: usize, max_entry_bytes: usize) -> Self {
        Self {
            state: Arc::default(),
            max_bytes,
            max_entry_bytes: max_entry_bytes.min(max_bytes),
        }
    }

    /// Returns a cached source and marks it most recently used.
    pub fn get(&self, key: DecodedAudioCacheKey) -> Option<Arc<DecodedAudioSource>> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let source = Arc::clone(state.entries.get(&key)?);
        touch(&mut state.least_recent_first, key);
        Some(source)
    }

    /// Inserts a source if it fits the per-source and total cache limits.
    /// Existing entries win, preserving stable values for one source key.
    pub fn insert(
        &self,
        key: DecodedAudioCacheKey,
        source: Arc<DecodedAudioSource>,
    ) -> Option<Arc<DecodedAudioSource>> {
        let source_bytes = source.byte_len();
        if source_bytes > self.max_entry_bytes || source_bytes > self.max_bytes {
            return None;
        }
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(existing) = state.entries.get(&key).cloned() {
            touch(&mut state.least_recent_first, key);
            return Some(existing);
        }
        while state.bytes > self.max_bytes - source_bytes
            || state.entries.len() >= DECODED_AUDIO_CACHE_ENTRIES
        {
            let evictable = state.least_recent_first.iter().position(|candidate| {
                state
                    .entries
                    .get(candidate)
                    .is_some_and(|entry| Arc::strong_count(entry) == 1)
            });
            let index = evictable?;
            let oldest = state
                .least_recent_first
                .remove(index)
                .expect("cache eviction index came from queue");
            if let Some(evicted) = state.entries.remove(&oldest) {
                state.bytes = state.bytes.saturating_sub(evicted.byte_len());
            }
        }
        state.bytes += source_bytes;
        state.entries.insert(key, Arc::clone(&source));
        state.least_recent_first.push_back(key);
        Some(source)
    }

    /// Returns decoded bytes currently held by the cache.
    pub fn cached_bytes(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .bytes
    }
}

fn touch(order: &mut VecDeque<DecodedAudioCacheKey>, key: DecodedAudioCacheKey) {
    if let Some(index) = order.iter().position(|candidate| *candidate == key) {
        order.remove(index);
    }
    order.push_back(key);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(index: u8, sample_rate: u32) -> DecodedAudioCacheKey {
        DecodedAudioCacheKey {
            source_hash: [index; 32],
            output_sample_rate: sample_rate,
        }
    }

    fn source(byte_len: usize) -> Arc<DecodedAudioSource> {
        Arc::new(DecodedAudioSource::from_test_data(byte_len))
    }

    #[test]
    fn cache_hit_returns_the_same_decoded_source() {
        let cache = DecodedAudioCache::with_limits(16, 12);
        let source = source(8);
        cache.insert(key(1, 48_000), Arc::clone(&source)).unwrap();
        assert!(Arc::ptr_eq(&cache.get(key(1, 48_000)).unwrap(), &source));
        assert_eq!(cache.cached_bytes(), 8);
    }

    #[test]
    fn cache_miss_and_different_sample_rate_do_not_alias() {
        let cache = DecodedAudioCache::with_limits(16, 12);
        cache.insert(key(1, 48_000), source(8)).unwrap();
        assert!(cache.get(key(2, 48_000)).is_none());
        assert!(cache.get(key(1, 44_100)).is_none());
    }

    #[test]
    fn cache_evicts_least_recent_source_before_exceeding_budget() {
        let cache = DecodedAudioCache::with_limits(12, 8);
        cache.insert(key(1, 48_000), source(4)).unwrap();
        cache.insert(key(2, 48_000), source(4)).unwrap();
        cache.get(key(1, 48_000)).unwrap();
        cache.insert(key(3, 48_000), source(8)).unwrap();
        assert!(cache.get(key(1, 48_000)).is_some());
        assert!(cache.get(key(2, 48_000)).is_none());
        assert!(cache.get(key(3, 48_000)).is_some());
        assert_eq!(cache.cached_bytes(), 12);
    }

    #[test]
    fn cache_rejects_sources_over_entry_or_total_budget() {
        let cache = DecodedAudioCache::with_limits(8, 4);
        assert!(cache.insert(key(1, 48_000), source(5)).is_none());
        assert_eq!(cache.cached_bytes(), 0);
    }

    #[test]
    fn cache_does_not_evict_a_source_held_by_a_feeder() {
        let cache = DecodedAudioCache::with_limits(8, 8);
        let feeder_source = cache.insert(key(1, 48_000), source(8)).unwrap();
        assert!(cache.insert(key(2, 48_000), source(4)).is_none());
        assert!(Arc::ptr_eq(
            &cache.get(key(1, 48_000)).unwrap(),
            &feeder_source
        ));
        assert_eq!(cache.cached_bytes(), 8);
    }
}
