use crate::Value;
use std::collections::HashMap;

/// Hash index implementation optimized for equality lookups
pub struct HashIndex {
    map: HashMap<Value, Vec<usize>>,
}

impl HashIndex {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// Insert a key-value pair into the index
    /// Multiple row IDs can map to the same key value
    pub fn insert(&mut self, key: Value, row_id: usize) {
        self.map.entry(key).or_insert_with(Vec::new).push(row_id);
    }

    /// Lookup row IDs for an exact key match
    pub fn lookup(&self, key: &Value) -> Vec<usize> {
        self.map.get(key).cloned().unwrap_or_default()
    }

    /// Remove a specific row ID for a given key
    pub fn remove(&mut self, key: &Value, row_id: usize) {
        if let Some(row_ids) = self.map.get_mut(key) {
            row_ids.retain(|&id| id != row_id);
            if row_ids.is_empty() {
                self.map.remove(key);
            }
        }
    }

    /// Clear all entries from the index
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Get the number of unique keys in the index
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl Default for HashIndex {
    fn default() -> Self {
        Self::new()
    }
}

