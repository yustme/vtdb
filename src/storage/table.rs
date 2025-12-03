use crate::Value;
use anyhow::Result;

/// In-memory table storage using columnar format
pub struct Table {
    name: String,
    columns: Vec<Column>,
    row_count: usize,
}

/// Columnar column storage
struct Column {
    data: Vec<Value>,
}

impl Table {
    pub fn new(name: String, column_count: usize) -> Self {
        let columns = (0..column_count)
            .map(|_| Column { data: Vec::new() })
            .collect();

        Self {
            name,
            columns,
            row_count: 0,
        }
    }

    pub fn insert_row(&mut self, row: Vec<Value>) -> Result<()> {
        if row.len() != self.columns.len() {
            return Err(anyhow::anyhow!(
                "Row has {} columns, but table has {} columns",
                row.len(),
                self.columns.len()
            ));
        }

        for (col_idx, value) in row.into_iter().enumerate() {
            self.columns[col_idx].data.push(value);
        }

        self.row_count += 1;
        Ok(())
    }

    pub fn scan_all(&self) -> Vec<Vec<Value>> {
        let mut rows = Vec::with_capacity(self.row_count);
        for row_idx in 0..self.row_count {
            let mut row = Vec::with_capacity(self.columns.len());
            for col in &self.columns {
                row.push(col.data[row_idx].clone());
            }
            rows.push(row);
        }
        rows
    }

    pub fn update_rows(
        &mut self,
        column_idx: usize,
        new_value: Value,
        predicate: impl Fn(&[Value]) -> bool,
    ) -> Result<usize> {
        if column_idx >= self.columns.len() {
            return Err(anyhow::anyhow!("Column index out of bounds"));
        }

        let mut updated = 0;
        for row_idx in 0..self.row_count {
            let row: Vec<Value> = self.columns.iter().map(|col| col.data[row_idx].clone()).collect();
            if predicate(&row) {
                self.columns[column_idx].data[row_idx] = new_value.clone();
                updated += 1;
            }
        }

        Ok(updated)
    }

    pub fn delete_rows(&mut self, predicate: impl Fn(&[Value]) -> bool) -> Result<usize> {
        let mut rows_to_keep: Vec<Vec<Value>> = Vec::new();
        let mut deleted = 0;

        // Collect rows to keep
        for row_idx in 0..self.row_count {
            let row: Vec<Value> = self.columns.iter().map(|col| col.data[row_idx].clone()).collect();
            if !predicate(&row) {
                rows_to_keep.push(row);
            } else {
                deleted += 1;
            }
        }

        // Rebuild columns
        self.row_count = rows_to_keep.len();
        for col_idx in 0..self.columns.len() {
            self.columns[col_idx].data = rows_to_keep.iter().map(|row| row[col_idx].clone()).collect();
        }

        Ok(deleted)
    }

    pub fn row_count(&self) -> usize {
        self.row_count
    }
}

