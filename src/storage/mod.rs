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
use iceberg::write_buffer::IcebergWriteBuffer;
use iceberg::config::IcebergWriteConfig;
use iceberg::manifest::DataFile;

/// Storage engine for managing table data
pub struct StorageEngine {
    tables: HashMap<String, table::Table>,
    wal: wal::WAL,
    index_manager: IndexManager,
    iceberg_catalog: Option<iceberg::catalog::IcebergCatalog>,
    iceberg_base_path: PathBuf,
    /// Write buffers per table for batching Iceberg writes
    iceberg_write_buffers: HashMap<String, IcebergWriteBuffer>,
    /// Configuration for Iceberg writes
    iceberg_write_config: IcebergWriteConfig,
    /// Pending data files per table (for manifest batching)
    pending_data_files: HashMap<String, Vec<DataFile>>,
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
            iceberg_write_buffers: HashMap::new(),
            iceberg_write_config: IcebergWriteConfig::default(),
            pending_data_files: HashMap::new(),
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
            iceberg_write_buffers: HashMap::new(),
            iceberg_write_config: IcebergWriteConfig::default(),
            pending_data_files: HashMap::new(),
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

        // Write to Iceberg for durability (buffered)
        // Note: If flush fails, data is still in memory, so we log but don't fail the insert
        // The flush will be retried on next insert or explicit flush
        if let Err(e) = self.write_to_iceberg_buffered(table_name, rows) {
            eprintln!("Warning: Failed to flush Iceberg buffer for table {}: {}. Data is still in memory and will be retried.", table_name, e);
            // Don't fail the insert - data is already in memory and can be flushed later
        }

        Ok(())
    }

    /// Write rows to Iceberg storage with buffering and batching
    fn write_to_iceberg_buffered(&mut self, table_name: &str, rows: Vec<Vec<Value>>) -> Result<()> {
        let catalog = self.ensure_iceberg_catalog()?;
        
        // Check if table exists in Iceberg catalog
        if !catalog.table_exists(table_name) {
            // Table doesn't exist in Iceberg yet - this shouldn't happen if table was created properly
            // Log warning but don't fail - data is still in memory
            eprintln!("Warning: Table '{}' does not exist in Iceberg catalog, skipping write. Data will only be in memory.", table_name);
            return Ok(());
        }

        // Get or create write buffer for this table
        let buffer = self.iceberg_write_buffers
            .entry(table_name.to_string())
            .or_insert_with(|| IcebergWriteBuffer::new(self.iceberg_write_config.clone()));

        // Add rows to buffer
        buffer.add_rows(rows);

        // Determine if we should flush:
        // 1. Small batches (< 1000 rows) - flush immediately for visibility
        // 2. Size threshold reached - flush for optimal file sizes
        // 3. Time threshold reached - flush to prevent data from being stuck in buffer
        // 4. Idle threshold - flush if no new inserts for N seconds
        // 5. Maximum age - force flush if buffer is older than max age (ensures < 10s)
        let row_count = buffer.row_count();
        let should_flush_immediately = row_count > 0 && row_count < 1000;
        let should_flush_by_size = buffer.should_flush_by_size();
        let should_flush_by_time = buffer.should_flush_by_time();
        let should_flush_by_idle = buffer.has_been_idle_for(self.iceberg_write_config.idle_flush_threshold_seconds);
        let should_flush_by_age = buffer.should_flush_by_age(self.iceberg_write_config.max_buffer_age_seconds);
        
        // Also flush if buffer has minimum rows and meets any threshold
        let has_minimum_rows = row_count >= self.iceberg_write_config.min_rows_for_flush;
        
        if should_flush_by_size || should_flush_immediately || should_flush_by_time 
            || should_flush_by_idle || should_flush_by_age
            || (has_minimum_rows && (should_flush_by_idle || should_flush_by_age)) {
            // Return error instead of silently ignoring - caller can decide how to handle
            self.flush_iceberg_buffer(table_name)
                .map_err(|e| anyhow::anyhow!("Failed to flush Iceberg buffer for table {}: {}", table_name, e))?;
        }

        Ok(())
    }

    /// Flush the write buffer for a table, writing Parquet files and creating manifests/snapshots
    fn flush_iceberg_buffer(&mut self, table_name: &str) -> Result<()> {
        // Peek at rows first to check if we have anything to write
        let row_count = {
            let buffer = self.iceberg_write_buffers.get(table_name)
                .ok_or_else(|| anyhow::anyhow!("No write buffer found for table {}", table_name))?;
            buffer.row_count()
        };
        
        if row_count == 0 {
            return Ok(());
        }

        // Clone rows before taking them, so we can restore on failure
        let rows_to_write = {
            let buffer = self.iceberg_write_buffers.get(table_name)
                .ok_or_else(|| anyhow::anyhow!("No write buffer found for table {}", table_name))?;
            buffer.peek_rows().to_vec()
        };

        // Now take rows from buffer (they're cloned, so we can restore if needed)
        let buffer = self.iceberg_write_buffers.get_mut(table_name)
            .ok_or_else(|| anyhow::anyhow!("No write buffer found for table {}", table_name))?;
        buffer.take_rows(); // Remove from buffer

        // Wrap the entire flush operation in a closure that can restore on error
        let flush_result = (|| -> Result<()> {
            let catalog = self.ensure_iceberg_catalog()?;
            
            // Get table metadata
            let metadata = catalog.get_table_metadata(table_name)?;
            let iceberg_schema = metadata.schema.clone();
            let data_dir = catalog.get_data_dir(table_name);
            
            // CRITICAL: Ensure data directory exists before writing files
            std::fs::create_dir_all(&data_dir)
                .map_err(|e| anyhow::anyhow!("Failed to create data directory {:?}: {}", data_dir, e))?;
            
            let config = self.iceberg_write_config.clone();
            let parallelism = config.write_parallelism;
            let target_rows_per_file = config.estimate_rows_per_file();

            // If we have a lot of rows, split into multiple files and write in parallel
            let data_files = if rows_to_write.len() > target_rows_per_file * 2 && parallelism > 1 {
                // Split into chunks and write in parallel
                self.write_parquet_files_parallel(
                    &rows_to_write,
                    &iceberg_schema,
                    &data_dir,
                    target_rows_per_file,
                    parallelism,
                    &config,
                )?
            } else {
                // Single file write
                let file_id = uuid::Uuid::new_v4().to_string();
                let base_timestamp = chrono::Utc::now().timestamp_millis();
                let uuid_part = uuid::Uuid::new_v4().as_u128() as i64;
                let snapshot_id = base_timestamp * 1000 + (uuid_part % 1000);
                let parquet_file = data_dir.join(format!("{}-{}.parquet", snapshot_id, file_id));
                
                let record_count = iceberg::parquet_writer::ParquetWriter::write_rows_with_config(
                    &parquet_file,
                    &iceberg_schema,
                    rows_to_write.clone(),
                    &config,
                )?;

                // Verify file was actually created
                if !parquet_file.exists() {
                    return Err(anyhow::anyhow!("Parquet file was not created: {:?}", parquet_file));
                }

                let file_size = std::fs::metadata(&parquet_file)?.len() as i64;
                vec![iceberg::manifest::ManifestManager::create_data_file(
                    parquet_file.to_string_lossy().to_string(),
                    record_count as i64,
                    file_size,
                )]
            };

            let files_written = data_files.len();

            // Add all data files to pending list
            let pending_count = {
                let pending_files = self.pending_data_files
                    .entry(table_name.to_string())
                    .or_insert_with(Vec::new);
                pending_files.extend(data_files);
                pending_files.len()
            };

            // Update buffer counters and check for flushing
            let should_flush_manifest = pending_count >= self.iceberg_write_config.manifest_batch_size;
            let should_create_snapshot = {
                let buffer = self.iceberg_write_buffers.get_mut(table_name)
                    .ok_or_else(|| anyhow::anyhow!("No write buffer found for table {}", table_name))?;
                
                // Increment file counter for each file written
                for _ in 0..files_written {
                    buffer.increment_file_count();
                }
                
                buffer.should_create_snapshot()
            };

            // Track if manifest was already flushed to prevent double flush
            let mut manifest_flushed = false;

            // Flush manifest if needed
            if should_flush_manifest {
                self.flush_manifest_batch(table_name)?;
                manifest_flushed = true;
            }

            // Always flush manifests for small batches to ensure data is visible
            // For larger batches, only flush when threshold is reached
            let is_small_batch = files_written > 0 && files_written < self.iceberg_write_config.manifest_batch_size;
            
            // Ensure manifests are flushed before creating snapshot (but only if not already flushed)
            if (!manifest_flushed && pending_count > 0) || is_small_batch {
                self.flush_manifest_batch(table_name)?;
            }
            
            // Always create snapshot after flushing manifests to ensure data is visible
            // This is critical for data visibility - without snapshots, data files exist but aren't queryable
            if should_create_snapshot || is_small_batch || files_written > 0 {
                self.create_snapshot(table_name)?;
                let buffer = self.iceberg_write_buffers.get_mut(table_name)
                    .ok_or_else(|| anyhow::anyhow!("No write buffer found for table {}", table_name))?;
                buffer.reset_file_count();
            }

            Ok(())
        })();

        // If flush failed, restore rows to buffer
        if let Err(e) = &flush_result {
            let buffer = self.iceberg_write_buffers.get_mut(table_name)
                .ok_or_else(|| anyhow::anyhow!("No write buffer found for table {}", table_name))?;
            buffer.restore_rows(rows_to_write);
            return Err(anyhow::anyhow!("Failed to flush Iceberg buffer for table {}: {}", table_name, e));
        }

        flush_result
    }

    /// Write multiple Parquet files in parallel by splitting rows into chunks
    fn write_parquet_files_parallel(
        &self,
        rows: &[Vec<Value>],
        schema: &iceberg::schema::IcebergSchema,
        data_dir: &std::path::Path,
        target_rows_per_file: usize,
        parallelism: usize,
        config: &IcebergWriteConfig,
    ) -> Result<Vec<DataFile>> {
        use std::sync::mpsc;
        use std::thread;

        // Split rows into chunks
        let chunks: Vec<Vec<Vec<Value>>> = rows
            .chunks(target_rows_per_file)
            .map(|chunk| chunk.to_vec())
            .collect();

        if chunks.is_empty() {
            return Ok(Vec::new());
        }

        // Limit parallelism to number of chunks
        let actual_parallelism = parallelism.min(chunks.len());
        
        // Create channel for results
        let (tx, rx) = mpsc::channel();

        // Spawn threads to write files in parallel
        let mut handles = Vec::new();
        for (chunk_idx, chunk) in chunks.into_iter().enumerate() {
            let tx = tx.clone();
            let schema = schema.clone();
            let data_dir = data_dir.to_path_buf();
            let config = config.clone();

            let handle = thread::spawn(move || {
                // Ensure data directory exists in thread
                if let Err(e) = std::fs::create_dir_all(&data_dir) {
                    tx.send(Err(anyhow::anyhow!("Failed to create data directory {:?}: {}", data_dir, e))).unwrap_or_default();
                    return;
                }

                let file_id = uuid::Uuid::new_v4().to_string();
                let base_timestamp = chrono::Utc::now().timestamp_millis();
                let uuid_part = uuid::Uuid::new_v4().as_u128() as i64;
                let snapshot_id = base_timestamp * 1000 + (uuid_part % 1000);
                let parquet_file = data_dir.join(format!("{}-{}-{}.parquet", snapshot_id, chunk_idx, file_id));
                
                match iceberg::parquet_writer::ParquetWriter::write_rows_with_config(
                    &parquet_file,
                    &schema,
                    chunk,
                    &config,
                ) {
                    Ok(record_count) => {
                        // Verify file was actually created
                        if !parquet_file.exists() {
                            tx.send(Err(anyhow::anyhow!("Parquet file was not created: {:?}", parquet_file))).unwrap_or_default();
                            return;
                        }
                        
                        match std::fs::metadata(&parquet_file) {
                            Ok(metadata) => {
                                let file_size = metadata.len() as i64;
                                let data_file = iceberg::manifest::ManifestManager::create_data_file(
                                    parquet_file.to_string_lossy().to_string(),
                                    record_count as i64,
                                    file_size,
                                );
                                tx.send(Ok(data_file)).unwrap_or_default();
                            }
                            Err(e) => {
                                tx.send(Err(anyhow::anyhow!("Failed to get file metadata: {}", e))).unwrap_or_default();
                            }
                        }
                    }
                    Err(e) => {
                        tx.send(Err(anyhow::anyhow!("Failed to write Parquet file: {}", e))).unwrap_or_default();
                    }
                }
            });

            handles.push(handle);

            // Limit concurrent threads
            if handles.len() >= actual_parallelism {
                // Wait for one to complete before starting next
                if let Some(handle) = handles.pop() {
                    handle.join().unwrap_or_default();
                }
            }
        }

        // Wait for all remaining threads
        for handle in handles {
            handle.join().unwrap_or_default();
        }

        // Drop sender to close channel
        drop(tx);

        // Collect results
        let mut data_files = Vec::new();
        while let Ok(result) = rx.recv() {
            match result {
                Ok(data_file) => data_files.push(data_file),
                Err(e) => return Err(e),
            }
        }

        Ok(data_files)
    }

    /// Flush pending data files into a manifest
    /// This function is idempotent - calling it multiple times is safe
    fn flush_manifest_batch(&mut self, table_name: &str) -> Result<()> {
        let catalog = self.ensure_iceberg_catalog()?;
        let metadata_dir = catalog.get_metadata_dir(table_name);
        let manifest_manager = iceberg::manifest::ManifestManager::new(&metadata_dir);

        // Take pending files (idempotent - if already empty, return early)
        let pending_files = self.pending_data_files.remove(table_name)
            .unwrap_or_default();
        
        if pending_files.is_empty() {
            // Already flushed or no pending files - safe to return
            return Ok(());
        }

        // Use a combination of timestamp and random UUID to ensure uniqueness per table
        let base_timestamp = chrono::Utc::now().timestamp_millis();
        let uuid_part = uuid::Uuid::new_v4().as_u128() as i64;
        // Combine timestamp with UUID part to ensure uniqueness (use lower 32 bits of UUID)
        let snapshot_id = base_timestamp * 1000 + (uuid_part % 1000);
        let manifest_id = uuid::Uuid::new_v4().to_string();
        
        // Create manifest with all pending files
        let manifest_path = manifest_manager.create_manifest(
            &manifest_id,
            snapshot_id,
            pending_files.clone(),
        )?;

        // Create manifest list entry
        let manifest_list_entry = manifest_manager.create_manifest_list_entry(
            &manifest_path,
            snapshot_id,
            &pending_files,
        )?;

        // Find the latest manifest list by reading all manifest list files
        // and using the one with the highest snapshot_id, or create a new one
        let latest_snapshot_id = self.find_latest_manifest_list_snapshot(&metadata_dir)?;
        
        let mut all_entries = if let Some(latest_id) = latest_snapshot_id {
            // Read existing manifest list
            manifest_manager.read_manifest_list(latest_id).unwrap_or_default()
        } else {
            Vec::new()
        };
        
        all_entries.push(manifest_list_entry);

        // Write updated manifest list with new snapshot_id
        let _manifest_list_path = manifest_manager.write_manifest_list(
            snapshot_id,
            all_entries,
        )?;

        Ok(())
    }

    /// Find the latest manifest list snapshot ID by scanning manifest list files
    fn find_latest_manifest_list_snapshot(&self, metadata_dir: &std::path::Path) -> Result<Option<i64>> {
        if !metadata_dir.exists() {
            return Ok(None);
        }

        let mut max_snapshot_id: Option<i64> = None;

        for entry in std::fs::read_dir(metadata_dir)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();
            
            if file_name_str.starts_with("manifest-list-") && file_name_str.ends_with(".json") {
                if let Some(id_str) = file_name_str.strip_prefix("manifest-list-").and_then(|s| s.strip_suffix(".json")) {
                    if let Ok(snapshot_id) = id_str.parse::<i64>() {
                        max_snapshot_id = Some(max_snapshot_id.map_or(snapshot_id, |max| max.max(snapshot_id)));
                    }
                }
            }
        }

        Ok(max_snapshot_id)
    }

    /// Create a snapshot for the table
    fn create_snapshot(&mut self, table_name: &str) -> Result<()> {
        // Flush any remaining pending manifests first
        self.flush_manifest_batch(table_name)?;

        // Now get catalog and metadata
        let metadata_dir = {
            let catalog = self.ensure_iceberg_catalog()?;
            catalog.get_metadata_dir(table_name)
        };
        
        let current_schema_id = {
            let catalog = self.ensure_iceberg_catalog()?;
            let metadata = catalog.get_table_metadata(table_name)?;
            metadata.current_schema_id
        };

        // Find the latest manifest list snapshot ID
        let latest_snapshot_id = self.find_latest_manifest_list_snapshot(&metadata_dir)?
            .ok_or_else(|| anyhow::anyhow!("No manifest list found for table {}", table_name))?;
        
        let manifest_manager = iceberg::manifest::ManifestManager::new(&metadata_dir);
        
        // Read latest manifest list
        let manifest_entries = manifest_manager.read_manifest_list(latest_snapshot_id)
            .unwrap_or_default();
        
        if manifest_entries.is_empty() {
            return Ok(());
        }

        // Get the manifest list path from the latest snapshot ID
        let manifest_list_path = metadata_dir.join(format!("manifest-list-{}.json", latest_snapshot_id));

        // Create snapshot
        let mut snapshot_manager = iceberg::snapshot::SnapshotManager::new(&metadata_dir)?;
        snapshot_manager.create_snapshot(
            manifest_list_path.to_string_lossy().to_string(),
            "append".to_string(),
            current_schema_id,
        )?;

        Ok(())
    }

    /// Force flush all pending writes for a table (call before shutdown or table drop)
    pub fn flush_table_iceberg_writes(&mut self, table_name: &str) -> Result<()> {
        // Flush buffer if it has data
        let has_data = {
            let buffer = self.iceberg_write_buffers.get(table_name);
            buffer.map(|b| !b.is_empty()).unwrap_or(false)
        };
        
        if has_data {
            self.flush_iceberg_buffer(table_name)?;
        }

        // Flush any pending manifests
        self.flush_manifest_batch(table_name)?;

        // Create final snapshot
        self.create_snapshot(table_name)?;

        Ok(())
    }

    /// Check if a table has pending data in the write buffer
    pub fn has_pending_buffer_data(&self, table_name: &str) -> bool {
        self.iceberg_write_buffers
            .get(table_name)
            .map(|b| !b.is_empty())
            .unwrap_or(false)
    }

    /// Get the number of rows pending in the write buffer for a table
    pub fn get_pending_buffer_row_count(&self, table_name: &str) -> usize {
        self.iceberg_write_buffers
            .get(table_name)
            .map(|b| b.row_count())
            .unwrap_or(0)
    }

    /// Check if a table has pending data files waiting to be flushed to manifest
    pub fn has_pending_data_files(&self, table_name: &str) -> bool {
        self.pending_data_files
            .get(table_name)
            .map(|files| !files.is_empty())
            .unwrap_or(false)
    }

    /// Get the number of pending data files for a table
    pub fn get_pending_data_file_count(&self, table_name: &str) -> usize {
        self.pending_data_files
            .get(table_name)
            .map(|files| files.len())
            .unwrap_or(0)
    }

    /// Get list of tables with pending buffers
    pub fn get_tables_with_pending_buffers(&self) -> Vec<String> {
        self.iceberg_write_buffers
            .iter()
            .filter(|(_, buffer)| !buffer.is_empty())
            .map(|(table_name, _)| table_name.clone())
            .collect()
    }

    /// Flush all tables with pending buffers (for background task)
    pub fn flush_all_pending_buffers(&mut self) -> Result<()> {
        // Get list of tables with pending buffers
        let tables_to_flush = self.get_tables_with_pending_buffers();

        for table_name in tables_to_flush {
            // Check if buffer still needs flushing (might have been flushed by another process)
            let needs_flush = self.has_pending_buffer_data(&table_name);

            if needs_flush {
                // Check flush criteria
                let should_flush = {
                    let buffer = self.iceberg_write_buffers.get(&table_name);
                    if let Some(buffer) = buffer {
                        let row_count = buffer.row_count();
                        let has_minimum_rows = row_count >= self.iceberg_write_config.min_rows_for_flush;
                        let should_flush_by_size = buffer.should_flush_by_size();
                        let should_flush_by_time = buffer.should_flush_by_time();
                        let should_flush_by_idle = buffer.has_been_idle_for(self.iceberg_write_config.idle_flush_threshold_seconds);
                        let should_flush_by_age = buffer.should_flush_by_age(self.iceberg_write_config.max_buffer_age_seconds);

                        should_flush_by_size || should_flush_by_time || should_flush_by_idle 
                            || should_flush_by_age || (has_minimum_rows && (should_flush_by_idle || should_flush_by_age))
                    } else {
                        false
                    }
                };

                if should_flush {
                    if let Err(e) = self.flush_iceberg_buffer(&table_name) {
                        eprintln!("Warning: Background flush failed for table {}: {}", table_name, e);
                        // Continue with other tables even if one fails
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan all rows from a table
    pub fn scan_table(&mut self, table_name: &str) -> Result<Vec<Vec<Value>>> {
        // Flush any pending buffers before reading to ensure latest data is visible
        if self.has_pending_buffer_data(table_name) {
            if let Err(e) = self.flush_iceberg_buffer(table_name) {
                eprintln!("Warning: Failed to flush buffer before scan for table {}: {}", table_name, e);
                // Continue anyway - data is still in memory
            }
        }

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
        
        // Strategy: Use the latest snapshot to find the current manifest list,
        // or fall back to the latest manifest list if no snapshot exists
        // Then read only the manifests referenced in that list
        
        // Try to find the latest snapshot first
        let snapshot_manager = iceberg::snapshot::SnapshotManager::new(&metadata_dir)?;
        let latest_snapshot = snapshot_manager.get_latest_snapshot()?;
        
        let manifest_list_snapshot_id = if let Some(snapshot) = latest_snapshot {
            // Extract snapshot ID from manifest list path (e.g., "manifest-list-123456.json")
            if let Some(id_str) = snapshot.manifest_list
                .split('/')
                .last()
                .and_then(|s| s.strip_prefix("manifest-list-"))
                .and_then(|s| s.strip_suffix(".json"))
            {
                id_str.parse::<i64>().ok()
            } else {
                None
            }
        } else {
            None
        };
        
        // If we have a snapshot, use its manifest list; otherwise find the latest manifest list
        let latest_manifest_list_id = match manifest_list_snapshot_id
            .or_else(|| self.find_latest_manifest_list_snapshot(&metadata_dir).ok().flatten())
        {
            Some(id) => id,
            None => {
                // No manifest list found - table might be empty or not yet flushed
                return Ok(Vec::new());
            }
        };
        
        // Read the manifest list
        let manifest_entries = manifest_manager.read_manifest_list(latest_manifest_list_id)
            .unwrap_or_default();
        
        if manifest_entries.is_empty() {
            return Ok(Vec::new());
        }
        
        // Use a set to track which files we've already read (to avoid duplicates)
        use std::collections::HashSet;
        let mut read_files = HashSet::new();
        let mut all_rows = Vec::new();

        // Read each manifest referenced in the manifest list
        for entry in manifest_entries {
            let manifest_path = entry.manifest_path;
            if let Ok(manifest) = manifest_manager.read_manifest(&manifest_path) {
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
    pub fn scan_table_chunk(&mut self, table_name: &str, start_idx: usize, chunk_size: usize) -> Result<Vec<Vec<Value>>> {
        // Flush any pending buffers before reading to ensure latest data is visible
        if self.has_pending_buffer_data(table_name) {
            if let Err(e) = self.flush_iceberg_buffer(table_name) {
                eprintln!("Warning: Failed to flush buffer before chunk scan for table {}: {}", table_name, e);
                // Continue anyway - data is still in memory
            }
        }

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
    pub fn get_row_count(&mut self, table_name: &str) -> Result<usize> {
        // Flush any pending buffers before counting to ensure accurate row count
        if self.has_pending_buffer_data(table_name) {
            if let Err(e) = self.flush_iceberg_buffer(table_name) {
                eprintln!("Warning: Failed to flush buffer before row count for table {}: {}", table_name, e);
                // Continue anyway - we'll count what's available
            }
        }

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
    pub fn get_row_count_from_iceberg(&self, table_name: &str) -> Result<usize> {
        let catalog = self.iceberg_catalog.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Iceberg catalog not initialized")
        })?;

        let metadata_dir = catalog.get_metadata_dir(table_name);
        let manifest_manager = iceberg::manifest::ManifestManager::new(&metadata_dir);
        
        // Strategy: Use the latest snapshot to find the current manifest list,
        // or fall back to the latest manifest list if no snapshot exists
        // Then read only the manifests referenced in that list
        
        // Try to find the latest snapshot first
        let snapshot_manager = iceberg::snapshot::SnapshotManager::new(&metadata_dir)?;
        let latest_snapshot = snapshot_manager.get_latest_snapshot()?;
        
        let manifest_list_snapshot_id = if let Some(snapshot) = latest_snapshot {
            // Extract snapshot ID from manifest list path (e.g., "manifest-list-123456.json")
            if let Some(id_str) = snapshot.manifest_list
                .split('/')
                .last()
                .and_then(|s| s.strip_prefix("manifest-list-"))
                .and_then(|s| s.strip_suffix(".json"))
            {
                id_str.parse::<i64>().ok()
            } else {
                None
            }
        } else {
            None
        };
        
        // If we have a snapshot, use its manifest list; otherwise find the latest manifest list
        let latest_manifest_list_id = match manifest_list_snapshot_id
            .or_else(|| self.find_latest_manifest_list_snapshot(&metadata_dir).ok().flatten())
        {
            Some(id) => id,
            None => {
                // No manifest list found - table might be empty or not yet flushed
                return Ok(0);
            }
        };
        
        // Read the manifest list
        let manifest_entries = manifest_manager.read_manifest_list(latest_manifest_list_id)
            .unwrap_or_default();
        
        if manifest_entries.is_empty() {
            return Ok(0);
        }
        
        // Use a set to track which files we've already counted
        use std::collections::HashSet;
        let mut counted_files = HashSet::new();
        let mut total_rows = 0usize;
        
        // Read each manifest referenced in the manifest list
        for entry in manifest_entries {
            let manifest_path = entry.manifest_path;
            if let Ok(manifest) = manifest_manager.read_manifest(&manifest_path) {
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

    /// Get Iceberg write configuration
    pub fn iceberg_write_config(&self) -> &IcebergWriteConfig {
        &self.iceberg_write_config
    }

    /// Set Iceberg write configuration
    pub fn set_iceberg_write_config(&mut self, config: IcebergWriteConfig) {
        self.iceberg_write_config = config;
        // Note: Existing buffers will continue with old config until flushed
        // New buffers created after this will use the new config
    }

    /// Configure Iceberg write settings with a closure
    pub fn configure_iceberg_writes<F>(&mut self, f: F)
    where
        F: FnOnce(&mut IcebergWriteConfig),
    {
        f(&mut self.iceberg_write_config);
    }

    /// Drop all tables from memory and storage
    pub fn drop_all_tables(&mut self) -> Result<()> {
        // Get list of table names before clearing
        let table_names: Vec<String> = self.tables.keys().cloned().collect();

        // Flush all pending Iceberg writes
        for table_name in &table_names {
            let _ = self.flush_table_iceberg_writes(table_name);
        }

        // Clear in-memory tables
        self.tables.clear();

        // Clear indexes
        self.index_manager.clear_all();

        // Clear WAL
        self.wal = wal::WAL::new();

        // Clear write buffers
        self.iceberg_write_buffers.clear();
        self.pending_data_files.clear();

        // Delete all Iceberg tables from disk
        if let Some(catalog) = &mut self.iceberg_catalog {
            catalog.drop_all_tables()?;
        }

        Ok(())
    }
}

impl Default for StorageEngine {
    fn default() -> Self {
        Self::new()
    }
}

