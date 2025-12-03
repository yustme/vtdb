use crate::Value;
use crate::storage::StorageEngine;
use anyhow::Result;

/// Index scan operator for efficient table access using indexes
pub struct IndexScan;

impl IndexScan {
    /// Scan table by exact key match using an index
    pub fn scan_by_key(
        table: &str,
        column: &str,
        key: Value,
        storage: &StorageEngine,
    ) -> Result<Vec<usize>> {
        let index_manager = storage.index_manager();
        if let Some(index) = index_manager.get_index(table, column) {
            Ok(index.lookup(&key))
        } else {
            Ok(Vec::new()) // No index available
        }
    }

    /// Scan table by range using a B-tree index
    pub fn scan_by_range(
        table: &str,
        column: &str,
        min: Value,
        max: Value,
        storage: &StorageEngine,
    ) -> Result<Vec<usize>> {
        let index_manager = storage.index_manager();
        if let Some(index) = index_manager.get_index(table, column) {
            index.range_query(&min, &max)
                .ok_or_else(|| anyhow::anyhow!("Range queries require B-tree index"))
        } else {
            Ok(Vec::new()) // No index available
        }
    }

    /// Get rows by row IDs from a table
    pub fn get_rows_by_ids(
        table: &str,
        row_ids: &[usize],
        storage: &StorageEngine,
    ) -> Result<Vec<Vec<Value>>> {
        let all_rows = storage.scan_table(table)?;
        let mut result = Vec::new();
        for &row_id in row_ids {
            if row_id < all_rows.len() {
                result.push(all_rows[row_id].clone());
            }
        }
        Ok(result)
    }
}

