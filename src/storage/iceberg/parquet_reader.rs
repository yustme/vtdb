use anyhow::Result;
use arrow::datatypes::Schema;
use arrow::record_batch::RecordBatch;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use crate::Value;

/// Read data from Parquet format
pub struct ParquetReader;

impl ParquetReader {
    /// Read all rows from a Parquet file
    pub fn read_rows(file_path: impl AsRef<Path>) -> Result<Vec<Vec<Value>>> {
        let file = File::open(file_path)?;
        let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
        let mut reader = builder.build()?;

        let mut all_rows = Vec::new();

        // Read all batches
        while let Some(batch_result) = reader.next() {
            let batch = batch_result?;
            let rows = Self::record_batch_to_rows(&batch)?;
            all_rows.extend(rows);
        }

        Ok(all_rows)
    }

    /// Read rows with column projection (only read specified columns)
    pub fn read_rows_projected(
        file_path: impl AsRef<Path>,
        column_indices: &[usize],
    ) -> Result<Vec<Vec<Value>>> {
        let file = File::open(file_path)?;
        let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
        
        // Create projection mask
        let projection_mask = parquet::arrow::ProjectionMask::roots(
            builder.parquet_schema(), 
            column_indices.iter().copied()
        );
        let mut reader = builder.with_projection(projection_mask).build()?;

        let mut all_rows = Vec::new();

        while let Some(batch_result) = reader.next() {
            let batch = batch_result?;
            let rows = Self::record_batch_to_rows(&batch)?;
            all_rows.extend(rows);
        }

        Ok(all_rows)
    }

    /// Get schema from Parquet file
    pub fn read_schema(file_path: impl AsRef<Path>) -> Result<Arc<Schema>> {
        let file = File::open(file_path)?;
        let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
        Ok(builder.schema().clone())
    }

    /// Convert a RecordBatch to rows of Values
    fn record_batch_to_rows(batch: &RecordBatch) -> Result<Vec<Vec<Value>>> {
        let num_rows = batch.num_rows();
        let num_cols = batch.num_columns();
        
        let mut rows = Vec::with_capacity(num_rows);

        for row_idx in 0..num_rows {
            let mut row = Vec::with_capacity(num_cols);
            
            for col_idx in 0..num_cols {
                let array = batch.column(col_idx);
                let value = Self::array_value_to_value(array, row_idx)?;
                row.push(value);
            }
            
            rows.push(row);
        }

        Ok(rows)
    }

    /// Convert a single value from an Arrow array to a Value
    fn array_value_to_value(array: &Arc<dyn arrow::array::Array>, row_idx: usize) -> Result<Value> {
        use arrow::array::*;
        
        if array.is_null(row_idx) {
            return Ok(Value::Null);
        }

        match array.data_type() {
            arrow::datatypes::DataType::Int64 => {
                let array = array.as_any().downcast_ref::<Int64Array>().unwrap();
                Ok(Value::Integer(array.value(row_idx)))
            }
            arrow::datatypes::DataType::Utf8 => {
                let array = array.as_any().downcast_ref::<StringArray>().unwrap();
                Ok(Value::Varchar(array.value(row_idx).to_string()))
            }
            arrow::datatypes::DataType::Boolean => {
                let array = array.as_any().downcast_ref::<BooleanArray>().unwrap();
                Ok(Value::Boolean(array.value(row_idx)))
            }
            _ => Err(anyhow::anyhow!("Unsupported Arrow data type: {:?}", array.data_type())),
        }
    }
}

