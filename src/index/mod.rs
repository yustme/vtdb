pub mod btree;
pub mod hash;

use std::collections::HashMap;
use crate::Value;

pub use btree::BTreeIndex;
pub use hash::HashIndex;

/// Index type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexType {
    BTree,
    Hash,
}

/// Index enum that can hold either B-tree or Hash index
pub enum Index {
    BTree(BTreeIndex),
    Hash(HashIndex),
}

impl Index {
    /// Insert a key-value pair into the index
    pub fn insert(&mut self, key: Value, row_id: usize) {
        match self {
            Index::BTree(idx) => idx.insert(key, row_id),
            Index::Hash(idx) => idx.insert(key, row_id),
        }
    }

    /// Lookup row IDs for an exact key match
    pub fn lookup(&self, key: &Value) -> Vec<usize> {
        match self {
            Index::BTree(idx) => idx.lookup(key),
            Index::Hash(idx) => idx.lookup(key),
        }
    }

    /// Range query (only works for B-tree indexes)
    pub fn range_query(&self, min: &Value, max: &Value) -> Option<Vec<usize>> {
        match self {
            Index::BTree(idx) => Some(idx.range_query(min, max)),
            Index::Hash(_) => None, // Hash indexes don't support range queries
        }
    }

    /// Remove a specific row ID for a given key
    pub fn remove(&mut self, key: &Value, row_id: usize) {
        match self {
            Index::BTree(idx) => idx.remove(key, row_id),
            Index::Hash(idx) => idx.remove(key, row_id),
        }
    }

    /// Get the index type
    pub fn index_type(&self) -> IndexType {
        match self {
            Index::BTree(_) => IndexType::BTree,
            Index::Hash(_) => IndexType::Hash,
        }
    }
}

/// Index manager that coordinates indexes across all tables
pub struct IndexManager {
    // table_name -> (column_name -> Index)
    indexes: HashMap<String, HashMap<String, Index>>,
    // Track column indices for each table (table_name -> column_name -> column_index)
    column_indices: HashMap<String, HashMap<String, usize>>,
}

impl IndexManager {
    pub fn new() -> Self {
        Self {
            indexes: HashMap::new(),
            column_indices: HashMap::new(),
        }
    }

    /// Create an index on a table column
    pub fn create_index(&mut self, table: &str, column: &str, column_idx: usize, index_type: IndexType) {
        let index = match index_type {
            IndexType::BTree => Index::BTree(BTreeIndex::new()),
            IndexType::Hash => Index::Hash(HashIndex::new()),
        };

        self.indexes
            .entry(table.to_string())
            .or_insert_with(HashMap::new)
            .insert(column.to_string(), index);

        self.column_indices
            .entry(table.to_string())
            .or_insert_with(HashMap::new)
            .insert(column.to_string(), column_idx);
    }

    /// Get an index for a table column
    pub fn get_index(&self, table: &str, column: &str) -> Option<&Index> {
        self.indexes
            .get(table)?
            .get(column)
    }

    /// Get a mutable index for a table column
    pub fn get_index_mut(&mut self, table: &str, column: &str) -> Option<&mut Index> {
        self.indexes
            .get_mut(table)?
            .get_mut(column)
    }

    /// Get all indexes for a table
    pub fn get_table_indexes(&self, table: &str) -> Vec<(String, IndexType)> {
        self.indexes
            .get(table)
            .map(|cols| {
                cols.iter()
                    .map(|(col_name, idx)| (col_name.clone(), idx.index_type()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get column index for a table column
    pub fn get_column_index(&self, table: &str, column: &str) -> Option<usize> {
        self.column_indices
            .get(table)?
            .get(column)
            .copied()
    }

    /// Drop an index
    pub fn drop_index(&mut self, table: &str, column: &str) {
        if let Some(table_indexes) = self.indexes.get_mut(table) {
            table_indexes.remove(column);
        }
        if let Some(table_columns) = self.column_indices.get_mut(table) {
            table_columns.remove(column);
        }
    }

    /// Maintain indexes after inserting rows
    pub fn maintain_indexes_on_insert(&mut self, table: &str, row_id: usize, row: &[Value]) {
        if let Some(table_indexes) = self.indexes.get_mut(table) {
            for (column_name, index) in table_indexes.iter_mut() {
                if let Some(&col_idx) = self.column_indices
                    .get(table)
                    .and_then(|cols| cols.get(column_name))
                {
                    if col_idx < row.len() {
                        index.insert(row[col_idx].clone(), row_id);
                    }
                }
            }
        }
    }

    /// Maintain indexes after updating a row
    pub fn maintain_indexes_on_update(
        &mut self,
        table: &str,
        column_idx: usize,
        old_value: Value,
        new_value: Value,
        row_id: usize,
    ) {
        // Only update indexes if the value actually changed
        if old_value == new_value {
            return;
        }

        if let Some(table_indexes) = self.indexes.get_mut(table) {
            for (column_name, index) in table_indexes.iter_mut() {
                if let Some(&idx) = self.column_indices
                    .get(table)
                    .and_then(|cols| cols.get(column_name))
                {
                    if idx == column_idx {
                        // This is the column that changed
                        index.remove(&old_value, row_id);
                        index.insert(new_value.clone(), row_id);
                    }
                }
            }
        }
    }

    /// Maintain indexes after deleting a row
    pub fn maintain_indexes_on_delete(&mut self, table: &str, row_id: usize, row: &[Value]) {
        if let Some(table_indexes) = self.indexes.get_mut(table) {
            for (column_name, index) in table_indexes.iter_mut() {
                if let Some(&col_idx) = self.column_indices
                    .get(table)
                    .and_then(|cols| cols.get(column_name))
                {
                    if col_idx < row.len() {
                        index.remove(&row[col_idx], row_id);
                    }
                }
            }
        }
    }

    /// Rebuild all indexes for a table (used for bulk operations)
    pub fn rebuild_indexes_for_table(&mut self, table: &str, rows: &[Vec<Value>]) {
        if let Some(table_indexes) = self.indexes.get_mut(table) {
            // Clear all indexes first
            for index in table_indexes.values_mut() {
                match index {
                    Index::BTree(idx) => idx.clear(),
                    Index::Hash(idx) => idx.clear(),
                }
            }
        }

        // Rebuild indexes (need to release the mutable borrow first)
        for (row_id, row) in rows.iter().enumerate() {
            self.maintain_indexes_on_insert(table, row_id, row);
        }
    }
}

impl Default for IndexManager {
    fn default() -> Self {
        Self::new()
    }
}

