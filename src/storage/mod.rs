pub mod columnar;
pub mod table;
pub mod wal;
pub mod checkpoint;

use anyhow::Result;
use crate::Value;
use std::collections::HashMap;

/// Storage engine for managing table data
pub struct StorageEngine {
    tables: HashMap<String, table::Table>,
    wal: wal::WAL,
}

impl StorageEngine {
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
            wal: wal::WAL::new(),
        }
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
        table.insert_rows_batch(rows)?;
        
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

    /// Update rows in a table
    pub fn update_rows(
        &mut self,
        table_name: &str,
        column_idx: usize,
        new_value: Value,
        predicate: impl Fn(&[Value]) -> bool,
    ) -> Result<usize> {
        let new_value_clone = new_value.clone();
        let table = self.get_table(table_name)?;
        let updated = table.update_rows(column_idx, new_value, predicate)?;
        self.wal.append_update(table_name, column_idx, &new_value_clone)?;
        Ok(updated)
    }

    /// Delete rows from a table
    pub fn delete_rows(
        &mut self,
        table_name: &str,
        predicate: impl Fn(&[Value]) -> bool,
    ) -> Result<usize> {
        let table = self.get_table(table_name)?;
        let deleted = table.delete_rows(predicate)?;
        self.wal.append_delete(table_name)?;
        Ok(deleted)
    }
}

impl Default for StorageEngine {
    fn default() -> Self {
        Self::new()
    }
}

