use crate::Value;
use crate::storage::iceberg::config::IcebergWriteConfig;
use std::time::{SystemTime, UNIX_EPOCH};

/// Buffer for accumulating rows before writing to Iceberg
pub struct IcebergWriteBuffer {
    /// Accumulated rows
    rows: Vec<Vec<Value>>,
    /// Configuration
    config: IcebergWriteConfig,
    /// Timestamp when buffer was created or last flushed
    last_flush_time: u64,
    /// Timestamp when rows were last added to buffer
    last_insert_time: u64,
    /// Number of files written since last snapshot
    files_since_snapshot: usize,
}

impl IcebergWriteBuffer {
    /// Create a new write buffer
    pub fn new(config: IcebergWriteConfig) -> Self {
        let now = Self::current_timestamp();
        Self {
            rows: Vec::new(),
            config,
            last_flush_time: now,
            last_insert_time: now,
            files_since_snapshot: 0,
        }
    }

    /// Add rows to the buffer
    pub fn add_rows(&mut self, rows: Vec<Vec<Value>>) {
        self.rows.extend(rows);
        self.last_insert_time = Self::current_timestamp();
    }

    /// Check if buffer should be flushed based on size
    pub fn should_flush_by_size(&self) -> bool {
        if self.rows.is_empty() {
            return false;
        }

        // Estimate current buffer size
        let estimated_size = self.rows.len() * self.config.estimated_bytes_per_row;
        estimated_size >= self.config.target_file_size_bytes
    }

    /// Check if buffer should be flushed based on time (for snapshots)
    pub fn should_flush_by_time(&self) -> bool {
        let current_time = Self::current_timestamp();
        let elapsed = current_time.saturating_sub(self.last_flush_time);
        elapsed >= self.config.snapshot_interval_seconds
    }

    /// Check if buffer has been idle (no new inserts) for N seconds
    pub fn has_been_idle_for(&self, seconds: u64) -> bool {
        let current_time = Self::current_timestamp();
        let elapsed = current_time.saturating_sub(self.last_insert_time);
        elapsed >= seconds
    }

    /// Get the age of the buffer in seconds (time since last insert)
    pub fn get_buffer_age_seconds(&self) -> u64 {
        let current_time = Self::current_timestamp();
        current_time.saturating_sub(self.last_insert_time)
    }

    /// Check if buffer should be flushed based on maximum age
    pub fn should_flush_by_age(&self, max_age_seconds: u64) -> bool {
        if self.rows.is_empty() {
            return false;
        }
        self.get_buffer_age_seconds() >= max_age_seconds
    }

    /// Check if a snapshot should be created
    pub fn should_create_snapshot(&self) -> bool {
        self.files_since_snapshot >= self.config.snapshot_interval_files
            || self.should_flush_by_time()
    }

    /// Take all rows from the buffer (for flushing)
    pub fn take_rows(&mut self) -> Vec<Vec<Value>> {
        let rows = std::mem::take(&mut self.rows);
        self.last_flush_time = Self::current_timestamp();
        rows
    }

    /// Get rows without taking them (for checking)
    pub fn peek_rows(&self) -> &[Vec<Value>] {
        &self.rows
    }

    /// Restore rows back to the buffer (for rollback on failure)
    pub fn restore_rows(&mut self, rows: Vec<Vec<Value>>) {
        // Prepend restored rows to maintain order
        let mut restored = rows;
        restored.extend(std::mem::take(&mut self.rows));
        self.rows = restored;
    }

    /// Increment file counter
    pub fn increment_file_count(&mut self) {
        self.files_since_snapshot += 1;
    }

    /// Reset file counter (after snapshot)
    pub fn reset_file_count(&mut self) {
        self.files_since_snapshot = 0;
        let now = Self::current_timestamp();
        self.last_flush_time = now;
        // Don't reset last_insert_time - we want to track when data was actually added
    }

    /// Get current file count
    pub fn files_since_snapshot(&self) -> usize {
        self.files_since_snapshot
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Get current buffer size estimate
    pub fn estimated_size(&self) -> usize {
        self.rows.len() * self.config.estimated_bytes_per_row
    }

    /// Get number of rows in buffer
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Get current timestamp in seconds since epoch
    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

