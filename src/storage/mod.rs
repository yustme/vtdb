pub mod columnar;
pub mod table;
pub mod wal;
pub mod checkpoint;

use anyhow::Result;
use crate::Value;
use crate::index::IndexManager;
use std::collections::HashMap;

/// Storage engine for managing table data
pub struct StorageEngine {
    tables: HashMap<String, table::Table>,
    wal: wal::WAL,
    index_manager: IndexManager,
}

impl StorageEngine {
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
            wal: wal::WAL::new(),
            index_manager: IndexManager::new(),
        }
    }

    /// Get reference to index manager
    pub fn index_manager(&self) -> &IndexManager {
        &self.index_manager
    }

    /// Get mutable reference to index manager
    pub fn index_manager_mut(&mut self) -> &mut IndexManager {
        &mut self.index_manager
    }

    /// Create a new table
    pub fn create_table(&mut self, name: String, column_count: usize) -> Result<()> {
        let table = table::Table::new(name.clone(), column_count);
        self.tables.insert(name, table);
        Ok(())
    }

    /// Get table reference
    pub fn get_table(&mut self, name: &str) -> Result<&mut table::Table> {
        self.tables.get_mut(name).ok_or_else(|| {
            anyhow::anyhow!("Table '{}' not found", name)
        })
    }

    /// Insert rows into a table using optimized batch insertion
    pub fn insert_rows(&mut self, table_name: &str, rows: Vec<Vec<Value>>) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }

        // Clone rows for WAL before batch insertion (WAL needs to own the data)
        // This clone is necessary for durability - WAL must have its own copy
        let rows_for_wal = rows.clone();
        
        // Use batch insertion for better performance
        let table = self.get_table(table_name)?;
        let start_row_id = table.row_count();
        table.insert_rows_batch(rows.clone())?;
        
        // Maintain indexes efficiently for bulk inserts
        // For large batches (>1000 rows), defer index updates
        if rows.len() > 1000 {
            // Defer: rebuild indexes after insert
            // For now, we'll still update incrementally but could optimize further
        }
        
        // Update indexes incrementally
        for (offset, row) in rows.iter().enumerate() {
            let row_id = start_row_id + offset;
            self.index_manager.maintain_indexes_on_insert(table_name, row_id, row);
        }
        
        // Write to WAL once per batch (already optimized - single write per batch)
        // WAL writes are batched at the insert_rows() level, not per-row
        self.wal.append_insert(table_name, rows_for_wal)?;
        Ok(())
    }

    /// Scan all rows from a table
    pub fn scan_table(&self, table_name: &str) -> Result<Vec<Vec<Value>>> {
        let table = self.tables.get(table_name).ok_or_else(|| {
            anyhow::anyhow!("Table '{}' not found", table_name)
        })?;
        Ok(table.scan_all())
    }

    /// Scan a chunk of rows from a table
    pub fn scan_table_chunk(&self, table_name: &str, start_idx: usize, chunk_size: usize) -> Result<Vec<Vec<Value>>> {
        let table = self.tables.get(table_name).ok_or_else(|| {
            anyhow::anyhow!("Table '{}' not found", table_name)
        })?;
        Ok(table.scan_chunk(start_idx, chunk_size))
    }

    /// Update rows in a table
    pub fn update_rows(
        &mut self,
        table_name: &str,
        column_idx: usize,
        new_value: Value,
        predicate: impl Fn(&[Value]) -> bool,
    ) -> Result<usize> {
        let new_value_clone = new_value.clone();
        
        // Collect rows that will be updated with their old values for index maintenance (before update)
        let updates: Vec<(usize, Value)> = {
            let table = self.tables.get(table_name).ok_or_else(|| {
                anyhow::anyhow!("Table '{}' not found", table_name)
            })?;
            let mut result = Vec::new();
            for row_idx in 0..table.row_count() {
                if let Some(row) = table.get_row(row_idx) {
                    if predicate(&row) {
                        let old_value = row[column_idx].clone();
                        result.push((row_idx, old_value));
                    }
                }
            }
            result
        };
        
        // Perform the update
        let table = self.get_table(table_name)?;
        let updated = table.update_rows(column_idx, new_value, predicate)?;
        
        // Maintain indexes: remove old entries and add new entries
        for (row_id, old_value) in updates {
            self.index_manager.maintain_indexes_on_update(
                table_name,
                column_idx,
                old_value,
                new_value_clone.clone(),
                row_id,
            );
        }
        
        self.wal.append_update(table_name, column_idx, &new_value_clone)?;
        Ok(updated)
    }

    /// Delete rows from a table
    pub fn delete_rows(
        &mut self,
        table_name: &str,
        predicate: impl Fn(&[Value]) -> bool,
    ) -> Result<usize> {
        // Collect rows that will be deleted for index maintenance (before delete)
        let rows_to_delete: Vec<(usize, Vec<Value>)> = {
            let table = self.tables.get(table_name).ok_or_else(|| {
                anyhow::anyhow!("Table '{}' not found", table_name)
            })?;
            let mut result = Vec::new();
            for row_idx in 0..table.row_count() {
                if let Some(row) = table.get_row(row_idx) {
                    if predicate(&row) {
                        result.push((row_idx, row));
                    }
                }
            }
            result
        };
        
        // Perform the delete (this rebuilds the table, so row IDs change)
        let table = self.get_table(table_name)?;
        let deleted = table.delete_rows(predicate)?;
        
        // Since delete_rows rebuilds the table, we need to rebuild indexes
        // Get all remaining rows and rebuild indexes
        let remaining_rows = {
            let table_ref = self.tables.get(table_name).ok_or_else(|| {
                anyhow::anyhow!("Table '{}' not found", table_name)
            })?;
            table_ref.scan_all()
        };
        self.index_manager.rebuild_indexes_for_table(table_name, &remaining_rows);
        
        self.wal.append_delete(table_name)?;
        Ok(deleted)
    }

    /// Get row count for a table
    pub fn get_row_count(&self, table_name: &str) -> Result<usize> {
        let table = self.tables.get(table_name).ok_or_else(|| {
            anyhow::anyhow!("Table '{}' not found", table_name)
        })?;
        Ok(table.row_count())
    }

    /// Estimate storage size in bytes for a table
    pub fn estimate_storage_size(&self, table_name: &str) -> Result<usize> {
        let table = self.tables.get(table_name).ok_or_else(|| {
            anyhow::anyhow!("Table '{}' not found", table_name)
        })?;
        Ok(table.estimate_size())
    }
}

impl Default for StorageEngine {
    fn default() -> Self {
        Self::new()
    }
}

