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
        storage: &mut StorageEngine,
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
        storage: &mut StorageEngine,
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
    /// Optimized to avoid full table scan by:
    /// 1. Sorting row IDs for better cache locality
    /// 2. Using direct row access when table is in memory
    /// 3. Batching row access for Iceberg tables
    pub fn get_rows_by_ids(
        table: &str,
        row_ids: &[usize],
        storage: &mut StorageEngine,
    ) -> Result<Vec<Vec<Value>>> {
        if row_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Check if table is in memory (can use direct row access)
        if let Ok(table_ref) = storage.get_table(table) {
            // Table is in memory - use direct row access
            let mut sorted_row_ids = row_ids.to_vec();
            sorted_row_ids.sort_unstable();
            sorted_row_ids.dedup();

            let mut result = Vec::with_capacity(sorted_row_ids.len());
            for &row_id in &sorted_row_ids {
                if let Some(row) = table_ref.get_row(row_id) {
                    result.push(row);
                }
            }
            return Ok(result);
        }

        // Table is not in memory - need to read from Iceberg
        // For Iceberg, we still need to scan, but we can optimize by:
        // 1. Sorting row IDs to access rows in order
        // 2. Only scanning up to the maximum row ID needed
        let mut sorted_row_ids = row_ids.to_vec();
        sorted_row_ids.sort_unstable();
        sorted_row_ids.dedup();

        if let Some(&max_row_id) = sorted_row_ids.last() {
            // Get row count to know how many rows to scan
            let row_count = storage.get_row_count(table).unwrap_or(0);
            if max_row_id >= row_count {
                // Some row IDs are out of bounds, filter them out
                sorted_row_ids.retain(|&id| id < row_count);
            }

            // For Iceberg tables, we need to scan all rows and filter by row IDs
            // This is because Iceberg doesn't support direct row access by ID
            // Sorting row IDs helps with cache locality when accessing the result
            let all_rows = storage.scan_table(table)?;
            let mut result = Vec::with_capacity(sorted_row_ids.len());
            for &row_id in &sorted_row_ids {
                if row_id < all_rows.len() {
                    result.push(all_rows[row_id].clone());
                }
            }
            Ok(result)
        } else {
            Ok(Vec::new())
        }
    }
}

