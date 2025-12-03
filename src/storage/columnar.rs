use crate::Value;

/// Columnar storage format utilities
/// This module provides utilities for columnar data operations

/// Columnar batch for vectorized operations
pub struct ColumnarBatch {
    pub columns: Vec<Vec<Value>>,
    pub row_count: usize,
}

impl ColumnarBatch {
    pub fn new(column_count: usize) -> Self {
        Self {
            columns: vec![Vec::new(); column_count],
            row_count: 0,
        }
    }

    pub fn add_row(&mut self, row: Vec<Value>) {
        if row.len() != self.columns.len() {
            panic!("Row length mismatch");
        }
        for (col_idx, value) in row.into_iter().enumerate() {
            self.columns[col_idx].push(value);
        }
        self.row_count += 1;
    }

    pub fn get_row(&self, row_idx: usize) -> Vec<Value> {
        self.columns.iter().map(|col| col[row_idx].clone()).collect()
    }
}

