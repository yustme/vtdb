use anyhow::Result;
use arrow::array::*;
use arrow::datatypes::*;
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use crate::Value;
use crate::storage::iceberg::schema::IcebergSchema;

/// Write data to Parquet format
pub struct ParquetWriter;

impl ParquetWriter {
    /// Write rows to a Parquet file
    pub fn write_rows(
        file_path: impl AsRef<Path>,
        schema: &IcebergSchema,
        rows: Vec<Vec<Value>>,
    ) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }

        // Convert Iceberg schema to Arrow schema
        let arrow_schema = Arc::new(Self::iceberg_to_arrow_schema(schema)?);

        // Convert rows to Arrow arrays
        let arrays = Self::rows_to_arrow_arrays(&arrow_schema, rows)?;

        // Create record batch
        let record_batch = RecordBatch::try_new(arrow_schema.clone(), arrays)?;

        // Write to Parquet file
        let file = File::create(file_path)?;
        let props = WriterProperties::builder().build();
        let mut writer = ArrowWriter::try_new(file, arrow_schema.clone(), Some(props))?;
        
        writer.write(&record_batch)?;
        writer.close()?;

        Ok(record_batch.num_rows())
    }

    /// Convert Iceberg schema to Arrow schema
    fn iceberg_to_arrow_schema(schema: &IcebergSchema) -> Result<Schema> {
        let fields: Vec<Field> = schema
            .fields
            .iter()
            .map(|field| {
                let arrow_type = match field.field_type {
                    crate::storage::iceberg::schema::IcebergType::Long => DataType::Int64,
                    crate::storage::iceberg::schema::IcebergType::String => DataType::Utf8,
                    crate::storage::iceberg::schema::IcebergType::Boolean => DataType::Boolean,
                };
                
                Field::new(
                    &field.name,
                    arrow_type,
                    !field.required, // nullable if not required
                )
            })
            .collect();

        Ok(Schema::new(fields))
    }

    /// Convert rows to Arrow arrays
    fn rows_to_arrow_arrays(
        arrow_schema: &Arc<Schema>,
        rows: Vec<Vec<Value>>,
    ) -> Result<Vec<Arc<dyn Array>>> {
        let num_cols = arrow_schema.fields().len();

        if num_cols == 0 {
            return Ok(vec![]);
        }

        let mut arrays: Vec<Box<dyn ArrayBuilder>> = (0..num_cols)
            .map(|i| {
                let field = arrow_schema.field(i);
                match field.data_type() {
                    DataType::Int64 => Ok(Box::new(Int64Builder::new()) as Box<dyn ArrayBuilder>),
                    DataType::Utf8 => Ok(Box::new(StringBuilder::new()) as Box<dyn ArrayBuilder>),
                    DataType::Boolean => Ok(Box::new(BooleanBuilder::new()) as Box<dyn ArrayBuilder>),
                    _ => Err(anyhow::anyhow!("Unsupported data type: {:?}", field.data_type())),
                }
            })
            .collect::<Result<Vec<_>>>()?;

        // Fill arrays with data
        for row in rows {
            if row.len() != num_cols {
                return Err(anyhow::anyhow!(
                    "Row has {} columns, but schema has {} columns",
                    row.len(),
                    num_cols
                ));
            }

            for (col_idx, value) in row.into_iter().enumerate() {
                match arrays[col_idx].as_mut() {
                    builder if builder.as_any().is::<Int64Builder>() => {
                        let builder = builder.as_any_mut().downcast_mut::<Int64Builder>().unwrap();
                        match value {
                            Value::Integer(i) => builder.append_value(i),
                            Value::Null => builder.append_null(),
                            _ => return Err(anyhow::anyhow!("Type mismatch: expected Integer, got {:?}", value)),
                        }
                    }
                    builder if builder.as_any().is::<StringBuilder>() => {
                        let builder = builder.as_any_mut().downcast_mut::<StringBuilder>().unwrap();
                        match value {
                            Value::Varchar(s) => builder.append_value(&s),
                            Value::Null => builder.append_null(),
                            _ => return Err(anyhow::anyhow!("Type mismatch: expected Varchar, got {:?}", value)),
                        }
                    }
                    builder if builder.as_any().is::<BooleanBuilder>() => {
                        let builder = builder.as_any_mut().downcast_mut::<BooleanBuilder>().unwrap();
                        match value {
                            Value::Boolean(b) => builder.append_value(b),
                            Value::Null => builder.append_null(),
                            _ => return Err(anyhow::anyhow!("Type mismatch: expected Boolean, got {:?}", value)),
                        }
                    }
                    _ => return Err(anyhow::anyhow!("Unsupported builder type")),
                }
            }
        }

        // Build arrays
        let arrays: Vec<Arc<dyn Array>> = arrays
            .into_iter()
            .map(|mut builder| builder.finish())
            .collect();

        Ok(arrays)
    }
}

