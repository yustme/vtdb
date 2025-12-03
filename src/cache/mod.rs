mod dependencies;

use std::collections::HashSet;
use lru::LruCache;
use crate::QueryResult;

pub use dependencies::extract_table_dependencies;

/// Cache entry storing query result and table dependencies
#[derive(Debug, Clone)]
struct CacheEntry {
    result: QueryResult,
    table_dependencies: HashSet<String>,
}

/// Query cache with LRU eviction
pub struct QueryCache {
    cache: LruCache<u64, CacheEntry>,
}

impl QueryCache {
    /// Create a new query cache with the specified capacity
    pub fn new(capacity: usize) -> Self {
        use std::num::NonZeroUsize;
        let cap = NonZeroUsize::new(capacity.max(1)).unwrap_or(NonZeroUsize::new(100).unwrap());
        Self {
            cache: LruCache::new(cap),
        }
    }

    /// Get cached query result if available
    pub fn get(&mut self, query_hash: u64) -> Option<QueryResult> {
        self.cache.get(&query_hash).map(|entry| entry.result.clone())
    }

    /// Store query result in cache with table dependencies
    pub fn put(&mut self, query_hash: u64, result: QueryResult, dependencies: HashSet<String>) {
        let entry = CacheEntry {
            result,
            table_dependencies: dependencies,
        };
        self.cache.put(query_hash, entry);
    }

    /// Invalidate cache entries that depend on any of the specified tables
    pub fn invalidate_tables(&mut self, tables: &[String]) {
        let tables_set: HashSet<String> = tables.iter().cloned().collect();
        
        // Collect keys to remove (can't remove during iteration)
        let keys_to_remove: Vec<u64> = self.cache
            .iter()
            .filter(|(_, entry)| {
                entry.table_dependencies.iter().any(|dep| tables_set.contains(dep))
            })
            .map(|(key, _)| *key)
            .collect();

        // Remove invalidated entries
        for key in keys_to_remove {
            let _ = self.cache.pop(&key);
        }
    }

    /// Clear all cache entries
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Get the number of entries in the cache
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.len() == 0
    }
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new(100)
    }
}

