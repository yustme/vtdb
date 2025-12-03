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

    /// Insert multiple rows efficiently using columnar batch insertion
    /// This method is optimized for batch inserts and avoids per-row overhead
    pub fn insert_rows_batch(&mut self, rows: Vec<Vec<Value>>) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }

        let row_count = rows.len();
        let expected_columns = self.columns.len();

        // Validate all rows have correct column count (single validation for entire batch)
        for (idx, row) in rows.iter().enumerate() {
            if row.len() != expected_columns {
                return Err(anyhow::anyhow!(
                    "Row {} has {} columns, but table has {} columns",
                    idx,
                    row.len(),
                    expected_columns
                ));
            }
        }

        // Pre-allocate capacity for all columns to avoid reallocations
        for col in &mut self.columns {
            col.data.reserve(row_count);
        }

        // Perform columnar batch insertion: extract values column by column
        // Note: We clone values here because we need to iterate over rows multiple times
        // (once per column). This is still more efficient than row-by-row insertion
        // because we pre-allocate and use extend() for bulk operations.
        for col_idx in 0..expected_columns {
            // Extract all values for this column from all rows
            let column_values: Vec<Value> = rows.iter()
                .map(|row| row[col_idx].clone())
                .collect();
            
            // Extend the column vector with all values at once (more efficient than multiple pushes)
            self.columns[col_idx].data.extend(column_values);
        }

        // Update row count
        self.row_count += row_count;
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

    /// Scan a chunk of rows starting from start_idx
    /// Returns rows from start_idx to start_idx + chunk_size (or end of table)
    pub fn scan_chunk(&self, start_idx: usize, chunk_size: usize) -> Vec<Vec<Value>> {
        let end_idx = (start_idx + chunk_size).min(self.row_count);
        if start_idx >= self.row_count {
            return Vec::new();
        }
        
        let chunk_size_actual = end_idx - start_idx;
        let mut rows = Vec::with_capacity(chunk_size_actual);
        
        for row_idx in start_idx..end_idx {
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

    /// Get a reference to columns (for index maintenance)
    pub fn get_column_data(&self, column_idx: usize) -> Option<&Vec<Value>> {
        self.columns.get(column_idx).map(|col| &col.data)
    }

    /// Get all columns (for index maintenance)
    pub fn get_columns(&self) -> &Vec<Column> {
        &self.columns
    }

    /// Get a row at a specific index
    pub fn get_row(&self, row_idx: usize) -> Option<Vec<Value>> {
        if row_idx >= self.row_count {
            return None;
        }
        Some(self.columns.iter().map(|col| col.data[row_idx].clone()).collect())
    }

    /// Estimate the memory size of the table in bytes
    pub fn estimate_size(&self) -> usize {
        let mut size = 0;
        
        // Size of table structure itself
        size += std::mem::size_of::<Table>();
        size += self.name.capacity();
        
        // Size of columns vector
        size += std::mem::size_of::<Vec<Column>>();
        size += self.columns.capacity() * std::mem::size_of::<Column>();
        
        // Size of each column's data
        for column in &self.columns {
            size += std::mem::size_of::<Vec<Value>>();
            size += column.data.capacity() * std::mem::size_of::<Value>();
            
            // Estimate size of actual values
            for value in &column.data {
                size += estimate_value_size(value);
            }
        }
        
        size
    }
}

/// Estimate the size of a Value in bytes
fn estimate_value_size(value: &Value) -> usize {
    match value {
        Value::Integer(_) => std::mem::size_of::<i64>(),
        Value::Varchar(s) => s.capacity(),
        Value::Boolean(_) => std::mem::size_of::<bool>(),
        Value::Null => 0,
    }
}

