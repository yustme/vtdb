use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Manifest file entry tracking data files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFile {
    pub manifest_path: String,
    pub manifest_length: i64,
    pub partition_spec_id: i32,
    pub added_snapshot_id: i64,
    pub added_data_files_count: i32,
    pub existing_data_files_count: i32,
    pub deleted_data_files_count: i32,
    pub added_rows_count: i64,
    pub existing_rows_count: i64,
    pub deleted_rows_count: i64,
    pub partitions: Vec<Partition>,
    pub added_files: Vec<DataFile>,
    pub existing_files: Vec<DataFile>,
    pub deleted_files: Vec<DataFile>,
}

/// Data file entry in manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFile {
    pub content: i32, // 0 = data, 1 = position deletes, 2 = equality deletes
    pub file_path: String,
    pub file_format: String, // "PARQUET"
    pub partition: HashMap<String, String>,
    pub record_count: i64,
    pub file_size_in_bytes: i64,
    pub column_sizes: Option<HashMap<i32, i64>>,
    pub value_counts: Option<HashMap<i32, i64>>,
    pub null_value_counts: Option<HashMap<i32, i64>>,
    pub nan_value_counts: Option<HashMap<i32, i64>>,
    pub lower_bounds: Option<HashMap<i32, String>>,
    pub upper_bounds: Option<HashMap<i32, String>>,
    pub key_metadata: Option<Vec<u8>>,
    pub split_offsets: Option<Vec<i64>>,
    pub equality_ids: Option<Vec<i32>>,
    pub sort_order_id: Option<i32>,
}

/// Partition information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Partition {
    pub partition_values: HashMap<String, String>,
}

/// Manifest list entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestListEntry {
    pub manifest_path: String,
    pub manifest_length: i64,
    pub partition_spec_id: i32,
    pub added_snapshot_id: i64,
    pub added_data_files_count: i32,
    pub existing_data_files_count: i32,
    pub deleted_data_files_count: i32,
    pub added_rows_count: i64,
    pub existing_rows_count: i64,
    pub deleted_rows_count: i64,
    pub partitions_summary: Option<PartitionSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionSummary {
    pub contains_null: bool,
    pub contains_nan: Option<bool>,
    pub lower_bound: Option<String>,
    pub upper_bound: Option<String>,
}

/// Manifest manager for Iceberg tables
pub struct ManifestManager {
    metadata_dir: PathBuf,
}

impl ManifestManager {
    /// Create a new manifest manager
    pub fn new(metadata_dir: impl AsRef<Path>) -> Self {
        Self {
            metadata_dir: metadata_dir.as_ref().to_path_buf(),
        }
    }

    /// Create a new manifest file
    pub fn create_manifest(
        &self,
        manifest_id: &str,
        snapshot_id: i64,
        data_files: Vec<DataFile>,
    ) -> Result<String> {
        let manifest_file = self.metadata_dir.join(format!("manifest-{}.json", manifest_id));
        
        let manifest = ManifestFile {
            manifest_path: manifest_file.to_string_lossy().to_string(),
            manifest_length: 0, // Will be updated after writing
            partition_spec_id: 0,
            added_snapshot_id: snapshot_id,
            added_data_files_count: data_files.len() as i32,
            existing_data_files_count: 0,
            deleted_data_files_count: 0,
            added_rows_count: data_files.iter().map(|f| f.record_count).sum(),
            existing_rows_count: 0,
            deleted_rows_count: 0,
            partitions: vec![],
            added_files: data_files,
            existing_files: vec![],
            deleted_files: vec![],
        };

        let json = serde_json::to_string_pretty(&manifest)?;
        let manifest_length = json.len() as i64;
        
        // Update manifest length
        let mut manifest = manifest;
        manifest.manifest_length = manifest_length;
        let json = serde_json::to_string_pretty(&manifest)?;
        
        fs::write(&manifest_file, json)?;

        Ok(manifest_file.to_string_lossy().to_string())
    }

    /// Read a manifest file
    pub fn read_manifest(&self, manifest_path: &str) -> Result<ManifestFile> {
        let path = if manifest_path.starts_with('/') || manifest_path.contains(':') {
            // Absolute path
            PathBuf::from(manifest_path)
        } else if manifest_path.starts_with("./") {
            // Relative path starting with ./
            let current_dir = std::env::current_dir()?;
            let relative_path = manifest_path.strip_prefix("./").unwrap_or(manifest_path);
            current_dir.join(relative_path)
        } else {
            // Relative path - resolve relative to metadata directory
            self.metadata_dir.join(manifest_path)
        };
        
        let json = fs::read_to_string(&path)?;
        let manifest: ManifestFile = serde_json::from_str(&json)?;
        Ok(manifest)
    }

    /// Create a manifest list entry
    pub fn create_manifest_list_entry(
        &self,
        manifest_path: &str,
        snapshot_id: i64,
        data_files: &[DataFile],
    ) -> Result<ManifestListEntry> {
        let manifest_length = fs::metadata(manifest_path)?.len() as i64;
        
        Ok(ManifestListEntry {
            manifest_path: manifest_path.to_string(),
            manifest_length,
            partition_spec_id: 0,
            added_snapshot_id: snapshot_id,
            added_data_files_count: data_files.len() as i32,
            existing_data_files_count: 0,
            deleted_data_files_count: 0,
            added_rows_count: data_files.iter().map(|f| f.record_count).sum(),
            existing_rows_count: 0,
            deleted_rows_count: 0,
            partitions_summary: None,
        })
    }

    /// Write manifest list
    pub fn write_manifest_list(
        &self,
        snapshot_id: i64,
        manifest_entries: Vec<ManifestListEntry>,
    ) -> Result<String> {
        let manifest_list_file = self.metadata_dir.join(format!("manifest-list-{}.json", snapshot_id));
        
        let json = serde_json::to_string_pretty(&manifest_entries)?;
        fs::write(&manifest_list_file, json)?;

        Ok(manifest_list_file.to_string_lossy().to_string())
    }

    /// Read manifest list
    pub fn read_manifest_list(&self, snapshot_id: i64) -> Result<Vec<ManifestListEntry>> {
        let manifest_list_file = self.metadata_dir.join(format!("manifest-list-{}.json", snapshot_id));
        
        if !manifest_list_file.exists() {
            return Ok(vec![]);
        }
        
        let json = fs::read_to_string(&manifest_list_file)?;
        let entries: Vec<ManifestListEntry> = serde_json::from_str(&json)?;
        Ok(entries)
    }

    /// Create a data file entry (static method)
    pub fn create_data_file(
        file_path: String,
        record_count: i64,
        file_size: i64,
    ) -> DataFile {
        DataFile {
            content: 0, // Data file
            file_path,
            file_format: "PARQUET".to_string(),
            partition: HashMap::new(),
            record_count,
            file_size_in_bytes: file_size,
            column_sizes: None,
            value_counts: None,
            null_value_counts: None,
            nan_value_counts: None,
            lower_bounds: None,
            upper_bounds: None,
            key_metadata: None,
            split_offsets: None,
            equality_ids: None,
            sort_order_id: None,
        }
    }
}

