use crate::catalog::types::DataType;
use serde::{Deserialize, Serialize};

/// Iceberg field type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IcebergType {
    Long,
    String,
    Boolean,
}

impl From<&DataType> for IcebergType {
    fn from(dt: &DataType) -> Self {
        match dt {
            DataType::Integer => IcebergType::Long,
            DataType::Varchar => IcebergType::String,
            DataType::Boolean => IcebergType::Boolean,
        }
    }
}

/// Iceberg field definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IcebergField {
    pub id: i32,
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: IcebergType,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// Iceberg schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IcebergSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    pub fields: Vec<IcebergField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_id: Option<i32>,
}

impl IcebergSchema {
    pub fn new(fields: Vec<IcebergField>) -> Self {
        Self {
            schema_type: "struct".to_string(),
            fields,
            schema_id: Some(0),
        }
    }

    /// Convert from catalog schema to Iceberg schema
    pub fn from_catalog_columns(columns: &[(String, DataType)]) -> Self {
        let fields: Vec<IcebergField> = columns
            .iter()
            .enumerate()
            .map(|(idx, (name, data_type))| IcebergField {
                id: idx as i32 + 1,
                name: name.clone(),
                field_type: IcebergType::from(data_type),
                required: false, // Allow nulls for now
                doc: None,
            })
            .collect();

        Self::new(fields)
    }

    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// Convert Iceberg schema to catalog column definitions
    pub fn to_catalog_columns(&self) -> Vec<(String, DataType)> {
        self.fields
            .iter()
            .map(|field| {
                let data_type = match field.field_type {
                    IcebergType::Long => DataType::Integer,
                    IcebergType::String => DataType::Varchar,
                    IcebergType::Boolean => DataType::Boolean,
                };
                (field.name.clone(), data_type)
            })
            .collect()
    }
}

