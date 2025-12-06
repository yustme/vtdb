pub mod columnar;
pub mod table;
pub mod wal;
pub mod checkpoint;
pub mod iceberg;

use anyhow::Result;
use crate::Value;
use crate::index::IndexManager;
use std::collections::HashMap;
use std::path::PathBuf;

/// Storage engine for managing table data
pub struct StorageEngine {
    tables: HashMap<String, table::Table>,
    wal: wal::WAL,
    index_manager: IndexManager,
    iceberg_catalog: Option<iceberg::catalog::IcebergCatalog>,
    iceberg_base_path: PathBuf,
}

impl StorageEngine {
    pub fn new() -> Self {
        let base_path = PathBuf::from("./data/iceberg");
        let mut engine = Self {
            tables: HashMap::new(),
            wal: wal::WAL::new(),
            index_manager: IndexManager::new(),
            iceberg_catalog: None,
            iceberg_base_path: base_path,
        };
        
        // Try to load existing tables from Iceberg (ignore errors if no tables exist)
        let _ = engine.load_existing_tables();
        
        engine
    }

    /// Create storage engine with custom Iceberg path
    pub fn with_iceberg_path(path: impl Into<PathBuf>) -> Result<Self> {
        let base_path = path.into();
        let catalog = iceberg::catalog::IcebergCatalog::new(&base_path)?;
        
        Ok(Self {
            tables: HashMap::new(),
            wal: wal::WAL::new(),
            index_manager: IndexManager::new(),
            iceberg_catalog: Some(catalog),
            iceberg_base_path: base_path,
        })
    }

    /// Initialize Iceberg catalog if not already initialized
    pub fn ensure_iceberg_catalog(&mut self) -> Result<&mut iceberg::catalog::IcebergCatalog> {
        if self.iceberg_catalog.is_none() {
            self.iceberg_catalog = Some(iceberg::catalog::IcebergCatalog::new(&self.iceberg_base_path)?);
        }
        Ok(self.iceberg_catalog.as_mut().unwrap())
    }

    /// Get reference to index manager
    pub fn index_manager(&self) -> &IndexManager {
        &self.index_manager
    }

    /// Get mutable reference to index manager
    pub fn index_manager_mut(&mut self) -> &mut IndexManager {
        &mut self.index_manager
    }

    /// Create a new table
    pub fn create_table(&mut self, name: String, column_count: usize) -> Result<()> {
        let table = table::Table::new(name.clone(), column_count);
        self.tables.insert(name, table);
        Ok(())
    }

    /// Create a new table with Iceberg support (with column definitions)
    pub fn create_table_with_schema(
        &mut self,
        name: String,
        columns: Vec<(String, crate::catalog::types::DataType)>,
    ) -> Result<()> {
        // Create in-memory table
        let table = table::Table::new(name.clone(), columns.len());
        self.tables.insert(name.clone(), table);

        // Create Iceberg table
        let catalog = self.ensure_iceberg_catalog()?;
        
        // Convert to Iceberg schema
        let iceberg_schema = iceberg::schema::IcebergSchema::from_catalog_columns(&columns);
        
        // Create table in Iceberg catalog
        catalog.create_table(&name, iceberg_schema)?;

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
        let start_row_id = table.row_count();
        table.insert_rows_batch(rows.clone())?;
        
        // Maintain indexes efficiently for bulk inserts
        // For large batches (>1000 rows), defer index updates
        if rows.len() > 1000 {
            // Defer: rebuild indexes after insert
            // For now, we'll still update incrementally but could optimize further
        }
        
        // Update indexes incrementally
        for (offset, row) in rows.iter().enumerate() {
            let row_id = start_row_id + offset;
            self.index_manager.maintain_indexes_on_insert(table_name, row_id, row);
        }
        
        // Write to WAL once per batch (already optimized - single write per batch)
        // WAL writes are batched at the insert_rows() level, not per-row
        self.wal.append_insert(table_name, rows_for_wal)?;

        // Write to Iceberg for durability
        self.write_to_iceberg(table_name, rows)?;

        Ok(())
    }

    /// Write rows to Iceberg storage
    fn write_to_iceberg(&mut self, table_name: &str, rows: Vec<Vec<Value>>) -> Result<()> {
        let catalog = self.ensure_iceberg_catalog()?;
        
        // Check if table exists in Iceberg catalog
        if !catalog.table_exists(table_name) {
            // Table doesn't exist in Iceberg yet, skip for now
            // It should have been created via create_table_with_schema
            return Ok(());
        }

        // Get table metadata
        let metadata = catalog.get_table_metadata(table_name)?;
        let iceberg_schema = &metadata.schema;

        // Generate file ID and snapshot ID
        let file_id = uuid::Uuid::new_v4().to_string();
        let snapshot_id = chrono::Utc::now().timestamp_millis();
        
        // Create data directory path
        let data_dir = catalog.get_data_dir(table_name);
        let parquet_file = data_dir.join(format!("{}-{}.parquet", snapshot_id, file_id));
        
        // Write Parquet file
        let record_count = iceberg::parquet_writer::ParquetWriter::write_rows(
            &parquet_file,
            iceberg_schema,
            rows.clone(),
        )?;

        // Get file size
        let file_size = std::fs::metadata(&parquet_file)?.len() as i64;

        // Create data file entry
        let manifest_manager = iceberg::manifest::ManifestManager::new(catalog.get_metadata_dir(table_name));
        let data_file = iceberg::manifest::ManifestManager::create_data_file(
            parquet_file.to_string_lossy().to_string(),
            record_count as i64,
            file_size,
        );

        // Create manifest
        let manifest_id = uuid::Uuid::new_v4().to_string();
        let manifest_path = manifest_manager.create_manifest(
            &manifest_id,
            snapshot_id,
            vec![data_file],
        )?;

        // Create manifest list entry
        let manifest_list_entry = manifest_manager.create_manifest_list_entry(
            &manifest_path,
            snapshot_id,
            &[iceberg::manifest::ManifestManager::create_data_file(
                parquet_file.to_string_lossy().to_string(),
                record_count as i64,
                file_size,
            )],
        )?;

        // Write manifest list
        let manifest_list_path = manifest_manager.write_manifest_list(
            snapshot_id,
            vec![manifest_list_entry],
        )?;

        // Create snapshot
        let mut snapshot_manager = iceberg::snapshot::SnapshotManager::new(catalog.get_metadata_dir(table_name))?;
        snapshot_manager.create_snapshot(
            manifest_list_path,
            "append".to_string(),
            metadata.current_schema_id,
        )?;

        Ok(())
    }

    /// Scan all rows from a table
    pub fn scan_table(&self, table_name: &str) -> Result<Vec<Vec<Value>>> {
        // If table exists in memory, always use it (it may have uncommitted changes like updates/deletes)
        // Only read from Iceberg if table doesn't exist in memory at all
        if let Some(table) = self.tables.get(table_name) {
            return Ok(table.scan_all());
        }

        // Table doesn't exist in memory, try to read from Iceberg
        if let Some(catalog) = &self.iceberg_catalog {
            if catalog.table_exists(table_name) {
                if let Ok(rows) = self.read_from_iceberg(table_name) {
                    return Ok(rows);
                }
            }
        }

        // Table not found anywhere
        Err(anyhow::anyhow!("Table '{}' not found", table_name))
    }

    /// Read rows from Iceberg storage
    fn read_from_iceberg(&self, table_name: &str) -> Result<Vec<Vec<Value>>> {
        let catalog = self.iceberg_catalog.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Iceberg catalog not initialized")
        })?;

        let metadata_dir = catalog.get_metadata_dir(table_name);
        let manifest_manager = iceberg::manifest::ManifestManager::new(&metadata_dir);
        
        // Strategy: Scan all manifest files and read data from unique data files
        // Use a set to track which files we've already read (to avoid duplicates)
        use std::collections::HashSet;
        let mut read_files = HashSet::new();
        let mut all_rows = Vec::new();

        // Find all manifest files in the metadata directory
        let manifest_files: Vec<_> = std::fs::read_dir(&metadata_dir)?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let file_name = entry.file_name();
                let file_name_str = file_name.to_string_lossy();
                if file_name_str.starts_with("manifest-") && file_name_str.ends_with(".json") {
                    Some(entry.path())
                } else {
                    None
                }
            })
            .collect();

        // Read each manifest and collect data files
        for manifest_path in manifest_files {
            let manifest_path_str = manifest_path.to_string_lossy().to_string();
            if let Ok(manifest) = manifest_manager.read_manifest(&manifest_path_str) {
                // Process added and existing files
                for data_file in manifest.added_files.iter().chain(manifest.existing_files.iter()) {
                    // Only read each file once (avoid duplicates)
                    if read_files.insert(data_file.file_path.clone()) {
                        // Resolve file path - handle both absolute and relative paths
                        let file_path = if data_file.file_path.starts_with('/') || data_file.file_path.contains(':') {
                            // Absolute path
                            std::path::PathBuf::from(&data_file.file_path)
                        } else if data_file.file_path.starts_with("./") {
                            // Relative path starting with ./ - resolve relative to current working directory
                            let current_dir = std::env::current_dir()?;
                            let relative_path = data_file.file_path.strip_prefix("./").unwrap_or(&data_file.file_path);
                            current_dir.join(relative_path)
                        } else {
                            // Relative path - try relative to table data directory first
                            let data_dir_path = catalog.get_data_dir(table_name).join(&data_file.file_path);
                            if data_dir_path.exists() {
                                data_dir_path
                            } else {
                                // Fallback to relative to table path
                                catalog.get_table_path(table_name).join(&data_file.file_path)
                            }
                        };

                        // Try to read the file
                        if file_path.exists() {
                            match iceberg::parquet_reader::ParquetReader::read_rows(&file_path) {
                                Ok(rows) => {
                                    all_rows.extend(rows);
                                }
                                Err(e) => {
                                    eprintln!("Warning: Failed to read Parquet file {:?}: {}", file_path, e);
                                    // Continue with other files
                                }
                            }
                        } else {
                            // Try alternative: just the filename in the data directory
                            if let Some(file_name) = std::path::Path::new(&data_file.file_path).file_name() {
                                let alt_path = catalog.get_data_dir(table_name).join(file_name);
                                if alt_path.exists() {
                                    match iceberg::parquet_reader::ParquetReader::read_rows(&alt_path) {
                                        Ok(rows) => {
                                            all_rows.extend(rows);
                                        }
                                        Err(e) => {
                                            eprintln!("Warning: Failed to read Parquet file {:?}: {}", alt_path, e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                
                // Note: We don't process deleted_files here because we're reading all files
                // Deleted files would be handled by snapshot isolation in a full implementation
            }
        }

        Ok(all_rows)
    }

    /// Scan a chunk of rows from a table
    pub fn scan_table_chunk(&self, table_name: &str, start_idx: usize, chunk_size: usize) -> Result<Vec<Vec<Value>>> {
        // Try Iceberg first
        if let Some(catalog) = &self.iceberg_catalog {
            if catalog.table_exists(table_name) {
                if let Ok(all_rows) = self.read_from_iceberg(table_name) {
                    let end_idx = (start_idx + chunk_size).min(all_rows.len());
                    if start_idx < all_rows.len() {
                        return Ok(all_rows[start_idx..end_idx].to_vec());
                    } else {
                        return Ok(Vec::new());
                    }
                }
            }
        }
        // Fallback to in-memory
        let table = self.tables.get(table_name).ok_or_else(|| {
            anyhow::anyhow!("Table '{}' not found", table_name)
        })?;
        Ok(table.scan_chunk(start_idx, chunk_size))
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
        
        // Collect rows that will be updated with their old values for index maintenance (before update)
        let updates: Vec<(usize, Value)> = {
            let table = self.tables.get(table_name).ok_or_else(|| {
                anyhow::anyhow!("Table '{}' not found", table_name)
            })?;
            let mut result = Vec::new();
            for row_idx in 0..table.row_count() {
                if let Some(row) = table.get_row(row_idx) {
                    if predicate(&row) {
                        let old_value = row[column_idx].clone();
                        result.push((row_idx, old_value));
                    }
                }
            }
            result
        };
        
        // Perform the update
        let table = self.get_table(table_name)?;
        let updated = table.update_rows(column_idx, new_value, predicate)?;
        
        // Maintain indexes: remove old entries and add new entries
        for (row_id, old_value) in updates {
            self.index_manager.maintain_indexes_on_update(
                table_name,
                column_idx,
                old_value,
                new_value_clone.clone(),
                row_id,
            );
        }
        
        self.wal.append_update(table_name, column_idx, &new_value_clone)?;
        Ok(updated)
    }

    /// Delete rows from a table
    pub fn delete_rows(
        &mut self,
        table_name: &str,
        predicate: impl Fn(&[Value]) -> bool,
    ) -> Result<usize> {
        // Collect rows that will be deleted for index maintenance (before delete)
        let rows_to_delete: Vec<(usize, Vec<Value>)> = {
            let table = self.tables.get(table_name).ok_or_else(|| {
                anyhow::anyhow!("Table '{}' not found", table_name)
            })?;
            let mut result = Vec::new();
            for row_idx in 0..table.row_count() {
                if let Some(row) = table.get_row(row_idx) {
                    if predicate(&row) {
                        result.push((row_idx, row));
                    }
                }
            }
            result
        };
        
        // Perform the delete (this rebuilds the table, so row IDs change)
        let table = self.get_table(table_name)?;
        let deleted = table.delete_rows(predicate)?;
        
        // Since delete_rows rebuilds the table, we need to rebuild indexes
        // Get all remaining rows and rebuild indexes
        let remaining_rows = {
            let table_ref = self.tables.get(table_name).ok_or_else(|| {
                anyhow::anyhow!("Table '{}' not found", table_name)
            })?;
            table_ref.scan_all()
        };
        self.index_manager.rebuild_indexes_for_table(table_name, &remaining_rows);
        
        self.wal.append_delete(table_name)?;
        Ok(deleted)
    }

    /// Get row count for a table
    pub fn get_row_count(&self, table_name: &str) -> Result<usize> {
        // Try to get row count from Iceberg storage first
        if let Some(catalog) = &self.iceberg_catalog {
            if catalog.table_exists(table_name) {
                if let Ok(count) = self.get_row_count_from_iceberg(table_name) {
                    return Ok(count);
                }
            }
        }

        // Fallback to in-memory table
        let table = self.tables.get(table_name).ok_or_else(|| {
            anyhow::anyhow!("Table '{}' not found", table_name)
        })?;
        Ok(table.row_count())
    }

    /// Get row count from Iceberg storage by reading manifests
    fn get_row_count_from_iceberg(&self, table_name: &str) -> Result<usize> {
        let catalog = self.iceberg_catalog.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Iceberg catalog not initialized")
        })?;

        let metadata_dir = catalog.get_metadata_dir(table_name);
        let manifest_manager = iceberg::manifest::ManifestManager::new(&metadata_dir);
        
        // Strategy: Scan all manifest files and sum record counts from unique data files
        // Use a set to track which files we've already counted
        use std::collections::HashSet;
        let mut counted_files = HashSet::new();
        let mut total_rows = 0usize;

        // Find all manifest files in the metadata directory
        let manifest_files: Vec<_> = std::fs::read_dir(&metadata_dir)?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let file_name = entry.file_name();
                let file_name_str = file_name.to_string_lossy();
                if file_name_str.starts_with("manifest-") && file_name_str.ends_with(".json") {
                    Some(entry.path())
                } else {
                    None
                }
            })
            .collect();

        // Read each manifest and sum record counts from unique files
        for manifest_path in manifest_files {
            let manifest_path_str = manifest_path.to_string_lossy().to_string();
            if let Ok(manifest) = manifest_manager.read_manifest(&manifest_path_str) {
                // Process added and existing files
                for data_file in manifest.added_files.iter().chain(manifest.existing_files.iter()) {
                    // Use file path as unique identifier
                    if counted_files.insert(data_file.file_path.clone()) {
                        total_rows += data_file.record_count as usize;
                    }
                }
                
                // Subtract deleted files (only if we've counted them before)
                for data_file in manifest.deleted_files.iter() {
                    if counted_files.contains(&data_file.file_path) {
                        total_rows = total_rows.saturating_sub(data_file.record_count as usize);
                        counted_files.remove(&data_file.file_path);
                    }
                }
            }
        }

        Ok(total_rows)
    }

    /// Estimate storage size in bytes for a table
    pub fn estimate_storage_size(&self, table_name: &str) -> Result<usize> {
        let table = self.tables.get(table_name).ok_or_else(|| {
            anyhow::anyhow!("Table '{}' not found", table_name)
        })?;
        Ok(table.estimate_size())
    }

    /// Load all existing tables from Iceberg storage
    pub fn load_existing_tables(&mut self) -> Result<()> {
        // Initialize catalog first
        let catalog = self.ensure_iceberg_catalog()?;
        
        // Discover all tables in the Iceberg catalog
        let table_names = catalog.list_tables();
        
        // Collect table info first to avoid borrow conflicts
        let mut tables_to_create = Vec::new();
        for table_name in table_names {
            // Load table metadata
            if catalog.load_table_metadata(&table_name).is_ok() {
                if let Ok(metadata) = catalog.get_table_metadata(&table_name) {
                    let column_count = metadata.schema.field_count();
                    tables_to_create.push((table_name, column_count));
                }
            }
        }
        
        // Now create in-memory table structures
        for (table_name, column_count) in tables_to_create {
            if !self.tables.contains_key(&table_name) {
                let table = table::Table::new(table_name.clone(), column_count);
                self.tables.insert(table_name, table);
            }
        }
        
        Ok(())
    }

    /// Get Iceberg catalog reference (for loading tables)
    pub fn iceberg_catalog(&self) -> Option<&iceberg::catalog::IcebergCatalog> {
        self.iceberg_catalog.as_ref()
    }

    /// Get mutable Iceberg catalog reference
    pub fn iceberg_catalog_mut(&mut self) -> Option<&mut iceberg::catalog::IcebergCatalog> {
        self.iceberg_catalog.as_mut()
    }
}

impl Default for StorageEngine {
    fn default() -> Self {
        Self::new()
    }
}

