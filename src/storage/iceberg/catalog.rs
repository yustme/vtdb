use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use serde::{Deserialize, Serialize};
use crate::storage::iceberg::schema::IcebergSchema;

/// Iceberg catalog for managing table metadata
pub struct IcebergCatalog {
    base_path: PathBuf,
    tables: HashMap<String, TableMetadata>,
}

/// Table metadata stored in Iceberg format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMetadata {
    pub format_version: i32,
    pub table_uuid: String,
    pub location: String,
    pub last_updated_ms: i64,
    pub last_column_id: i32,
    pub schema: IcebergSchema,
    pub partition_spec: Vec<PartitionSpec>,
    pub properties: HashMap<String, String>,
    pub current_schema_id: i32,
    pub schemas: Vec<SchemaEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionSpec {
    pub spec_id: i32,
    pub fields: Vec<PartitionField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionField {
    pub source_id: i32,
    pub field_id: i32,
    pub name: String,
    pub transform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaEntry {
    pub schema_id: i32,
    pub schema: IcebergSchema,
}

impl IcebergCatalog {
    /// Create a new Iceberg catalog with base path
    pub fn new(base_path: impl AsRef<Path>) -> Result<Self> {
        let base_path = base_path.as_ref().to_path_buf();
        
        // Create base directory if it doesn't exist
        if !base_path.exists() {
            fs::create_dir_all(&base_path)?;
        }

        Ok(Self {
            base_path,
            tables: HashMap::new(),
        })
    }

    /// Get the base path
    pub fn base_path(&self) -> &PathBuf {
        &self.base_path
    }

    /// Create a new table in the catalog
    pub fn create_table(
        &mut self,
        table_name: &str,
        schema: IcebergSchema,
    ) -> Result<TableMetadata> {
        let table_path = self.base_path.join(table_name);
        
        // Create table directory structure
        fs::create_dir_all(&table_path.join("data"))?;
        fs::create_dir_all(&table_path.join("metadata"))?;

        let table_uuid = uuid::Uuid::new_v4().to_string();
        let location = table_path.to_string_lossy().to_string();
        
        let metadata = TableMetadata {
            format_version: 1,
            table_uuid: table_uuid.clone(),
            location,
            last_updated_ms: chrono::Utc::now().timestamp_millis(),
            last_column_id: schema.field_count() as i32,
            schema: schema.clone(),
            partition_spec: vec![],
            properties: HashMap::new(),
            current_schema_id: 0,
            schemas: vec![SchemaEntry {
                schema_id: 0,
                schema,
            }],
        };

        // Write initial metadata file
        self.write_metadata_file(table_name, &metadata, 0)?;

        self.tables.insert(table_name.to_string(), metadata.clone());
        Ok(metadata)
    }

    /// Get table metadata
    pub fn get_table_metadata(&self, table_name: &str) -> Result<&TableMetadata> {
        self.tables.get(table_name).ok_or_else(|| {
            anyhow::anyhow!("Table '{}' not found in catalog", table_name)
        })
    }

    /// Get table metadata mutably
    pub fn get_table_metadata_mut(&mut self, table_name: &str) -> Result<&mut TableMetadata> {
        self.tables.get_mut(table_name).ok_or_else(|| {
            anyhow::anyhow!("Table '{}' not found in catalog", table_name)
        })
    }

    /// Update table metadata
    pub fn update_table_metadata(
        &mut self,
        table_name: &str,
        metadata: TableMetadata,
    ) -> Result<()> {
        // Get current version
        let version = if let Some(_existing) = self.tables.get(table_name) {
            self.get_latest_metadata_version(table_name)?
        } else {
            0
        };

        // Write new metadata version
        self.write_metadata_file(table_name, &metadata, version + 1)?;
        
        self.tables.insert(table_name.to_string(), metadata);
        Ok(())
    }

    /// Get table path
    pub fn get_table_path(&self, table_name: &str) -> PathBuf {
        self.base_path.join(table_name)
    }

    /// Get data directory for a table
    pub fn get_data_dir(&self, table_name: &str) -> PathBuf {
        self.get_table_path(table_name).join("data")
    }

    /// Get metadata directory for a table
    pub fn get_metadata_dir(&self, table_name: &str) -> PathBuf {
        self.get_table_path(table_name).join("metadata")
    }

    /// Write metadata file
    fn write_metadata_file(
        &self,
        table_name: &str,
        metadata: &TableMetadata,
        version: i32,
    ) -> Result<()> {
        let metadata_dir = self.get_metadata_dir(table_name);
        let metadata_file = metadata_dir.join(format!("v{}-metadata.json", version));
        
        let json = serde_json::to_string_pretty(metadata)?;
        fs::write(&metadata_file, json)?;
        
        Ok(())
    }

    /// Get latest metadata version
    fn get_latest_metadata_version(&self, table_name: &str) -> Result<i32> {
        let metadata_dir = self.get_metadata_dir(table_name);
        
        if !metadata_dir.exists() {
            return Ok(0);
        }

        let mut max_version = -1;
        for entry in fs::read_dir(&metadata_dir)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();
            
            if file_name_str.starts_with("v") && file_name_str.ends_with("-metadata.json") {
                if let Some(version_str) = file_name_str.strip_prefix("v").and_then(|s| s.strip_suffix("-metadata.json")) {
                    if let Ok(version) = version_str.parse::<i32>() {
                        max_version = max_version.max(version);
                    }
                }
            }
        }

        Ok(max_version.max(0))
    }

    /// Load table metadata from disk
    pub fn load_table_metadata(&mut self, table_name: &str) -> Result<()> {
        let metadata_dir = self.get_metadata_dir(table_name);
        
        if !metadata_dir.exists() {
            return Err(anyhow::anyhow!("Table '{}' metadata not found", table_name));
        }

        let version = self.get_latest_metadata_version(table_name)?;
        let metadata_file = metadata_dir.join(format!("v{}-metadata.json", version));
        
        let json = fs::read_to_string(&metadata_file)?;
        let metadata: TableMetadata = serde_json::from_str(&json)?;
        
        self.tables.insert(table_name.to_string(), metadata);
        Ok(())
    }

    /// Check if table exists
    pub fn table_exists(&self, table_name: &str) -> bool {
        self.tables.contains_key(table_name) || 
        self.get_table_path(table_name).exists()
    }

    /// List all tables
    pub fn list_tables(&self) -> Vec<String> {
        let mut tables = Vec::new();
        
        if let Ok(entries) = fs::read_dir(&self.base_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        tables.push(name.to_string());
                    }
                }
            }
        }
        
        tables
    }

    /// Drop all tables from Iceberg storage (delete directories from disk)
    pub fn drop_all_tables(&mut self) -> Result<()> {
        let table_names = self.list_tables();
        
        for table_name in table_names {
            let table_path = self.get_table_path(&table_name);
            if table_path.exists() {
                // Remove entire table directory (includes data/ and metadata/)
                if let Err(e) = fs::remove_dir_all(&table_path) {
                    eprintln!("Warning: Failed to delete table directory {:?}: {}", table_path, e);
                    // Continue with other tables even if one fails
                }
            }
        }

        // Clear internal table metadata cache
        self.tables.clear();

        Ok(())
    }
}

impl Default for TableMetadata {
    fn default() -> Self {
        Self {
            format_version: 1,
            table_uuid: uuid::Uuid::new_v4().to_string(),
            location: String::new(),
            last_updated_ms: chrono::Utc::now().timestamp_millis(),
            last_column_id: 0,
            schema: IcebergSchema::new(vec![]),
            partition_spec: vec![],
            properties: HashMap::new(),
            current_schema_id: 0,
            schemas: vec![],
        }
    }
}

