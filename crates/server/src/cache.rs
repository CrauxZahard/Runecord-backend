//! Bounded in-process caches.
//!
//! Runecord is one process on loopback, so "server cache" means a mutex-guarded,
//! bounded map in this process, not a second service. Callers fold a generation
//! into the key (SQLite `total_changes`) so any write invalidates by construction
//! instead of relying on timers.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::path::Path;

/// Guild detail is small; a few hundred entries cover every account that has
/// opened a server without keeping stale role data alive after a write.
pub const GUILD_CACHE_ENTRIES: usize = 256;
/// Decoded thumbnails are the one cache worth a byte budget: 32 MB holds a few
/// hundred 480 px images and stops a long session from pinning decoded pixels.
pub const THUMB_CACHE_BYTES: usize = 32 * 1024 * 1024;
pub const THUMB_CACHE_ENTRIES: usize = 256;
/// Thumbnails live on disk too; cap that directory so it cannot grow forever.
pub const THUMB_DISK_BYTES: u64 = 512 * 1024 * 1024;

/// A small LRU with both an entry cap and a weight cap.
///
/// `weight_of` is evaluated on stored values, so callers can budget bytes rather
/// than rows. Every operation is O(n) in the entry count, which is deliberate:
/// the caches here hold hundreds of entries, and a linked list would add more
/// code than it saves.
pub struct BoundedCache<K: Eq + Hash + Clone, V: Clone> {
    entries: HashMap<K, V>,
    order: VecDeque<K>,
    max_entries: usize,
    max_weight: usize,
    weight_of: fn(&V) -> usize,
    total_weight: usize,
    hits: u64,
    misses: u64,
}

impl<K: Eq + Hash + Clone, V: Clone> BoundedCache<K, V> {
    pub fn new(max_entries: usize, max_weight: usize, weight_of: fn(&V) -> usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            max_entries: max_entries.max(1),
            max_weight: max_weight.max(1),
            weight_of,
            total_weight: 0,
            hits: 0,
            misses: 0,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<V> {
        let Some(value) = self.entries.get(key).cloned() else {
            self.misses = self.misses.saturating_add(1);
            return None;
        };
        self.hits = self.hits.saturating_add(1);
        self.touch(key);
        Some(value)
    }

    pub fn put(&mut self, key: K, value: V) {
        let weight = (self.weight_of)(&value);
        if let Some(previous) = self.entries.insert(key.clone(), value) {
            self.total_weight = self.total_weight.saturating_sub((self.weight_of)(&previous));
        } else {
            self.order.push_back(key);
        }
        self.total_weight = self.total_weight.saturating_add(weight);
        self.evict();
    }

    fn touch(&mut self, key: &K) {
        if let Some(index) = self.order.iter().position(|entry| entry == key) {
            self.order.remove(index);
        }
        self.order.push_back(key.clone());
    }

    fn evict(&mut self) {
        while self.entries.len() > self.max_entries || self.total_weight > self.max_weight {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(value) = self.entries.remove(&oldest) {
                self.total_weight = self.total_weight.saturating_sub((self.weight_of)(&value));
            }
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.total_weight = 0;
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.entries.len(),
            weight: self.total_weight,
            hits: self.hits,
            misses: self.misses,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct CacheStats {
    pub entries: usize,
    pub weight: usize,
    pub hits: u64,
    pub misses: u64,
}

/// Deletes the oldest files in `dir` until the directory fits `max_bytes`.
///
/// Thumbnails are content-addressed and immutable, so age is the only useful
/// eviction signal. Files that cannot be statted are skipped rather than failing
/// the caller.
pub fn prune_dir(dir: &Path, max_bytes: u64) -> u64 {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut files = Vec::new();
    let mut total = 0u64;
    for entry in read_dir.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let modified = meta.modified().ok();
        total = total.saturating_add(meta.len());
        files.push((modified, entry.path(), meta.len()));
    }
    if total <= max_bytes {
        return 0;
    }
    files.sort_by_key(|(modified, _, _)| *modified);
    let mut removed = 0;
    for (_, path, len) in files {
        if total <= max_bytes {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_by_entries_and_weight() {
        let mut cache = BoundedCache::new(2, 100, |value: &Vec<u8>| value.len());
        cache.put("a".to_string(), vec![0u8; 10]);
        cache.put("b".to_string(), vec![0u8; 10]);
        assert!(cache.get(&"a".to_string()).is_some());
        cache.put("c".to_string(), vec![0u8; 10]);
        assert!(cache.get(&"b".to_string()).is_none());
        assert!(cache.get(&"a".to_string()).is_some());
        assert!(cache.get(&"c".to_string()).is_some());

        let mut weighted = BoundedCache::new(10, 25, |value: &Vec<u8>| value.len());
        weighted.put("a".to_string(), vec![0u8; 10]);
        weighted.put("b".to_string(), vec![0u8; 10]);
        weighted.put("c".to_string(), vec![0u8; 10]);
        let stats = weighted.stats();
        assert!(stats.weight <= 25);
        assert!(stats.entries <= 2);
    }
}
