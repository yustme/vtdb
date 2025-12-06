/// Configuration for Iceberg write operations
#[derive(Debug, Clone)]
pub struct IcebergWriteConfig {
    /// Target Parquet file size in bytes (default: 128MB)
    pub target_file_size_bytes: usize,
    /// Number of files per manifest (default: 10)
    pub manifest_batch_size: usize,
    /// Number of files per snapshot (default: 10)
    pub snapshot_interval_files: usize,
    /// Time-based snapshot interval in seconds (default: 30)
    pub snapshot_interval_seconds: u64,
    /// Parquet compression codec: "snappy", "zstd", "gzip", "lz4", "uncompressed" (default: "snappy")
    pub parquet_compression: String,
    /// Parquet row group size in bytes (default: 128MB)
    pub parquet_row_group_size: usize,
    /// Number of parallel writers (default: 4)
    pub write_parallelism: usize,
    /// Estimated bytes per row (used for buffering, default: 1000)
    pub estimated_bytes_per_row: usize,
    /// Maximum age in seconds before buffer is forced to flush (default: 10)
    pub max_buffer_age_seconds: u64,
    /// Idle flush threshold - flush if no inserts for N seconds (default: 3)
    pub idle_flush_threshold_seconds: u64,
    /// Minimum rows before considering flush (default: 100)
    pub min_rows_for_flush: usize,
}

impl Default for IcebergWriteConfig {
    fn default() -> Self {
        Self {
            target_file_size_bytes: 64 * 1024 * 1024, // 64MB (reduced for faster flushes)
            manifest_batch_size: 5, // Reduced for more frequent manifest flushes
            snapshot_interval_files: 10,
            snapshot_interval_seconds: 5, // Reduced from 30 to 5 seconds (well under 10s requirement)
            parquet_compression: "snappy".to_string(),
            parquet_row_group_size: 128 * 1024 * 1024, // 128MB
            write_parallelism: 4,
            estimated_bytes_per_row: 1000, // Conservative estimate
            max_buffer_age_seconds: 10, // Hard limit - force flush after 10 seconds
            idle_flush_threshold_seconds: 3, // Flush if idle for 3 seconds
            min_rows_for_flush: 100, // Minimum rows before considering flush
        }
    }
}

impl IcebergWriteConfig {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Estimate number of rows that would fit in target file size
    pub fn estimate_rows_per_file(&self) -> usize {
        if self.estimated_bytes_per_row == 0 {
            100_000 // Default fallback
        } else {
            self.target_file_size_bytes / self.estimated_bytes_per_row
        }
    }
}

