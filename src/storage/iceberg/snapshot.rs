use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Snapshot metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub sequence_number: i64,
    pub snapshot_id: i64,
    pub timestamp_ms: i64,
    pub summary: SnapshotSummary,
    pub manifest_list: String,
    pub schema_id: i32,
}

/// Snapshot summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotSummary {
    pub operation: String, // "append", "overwrite", "delete", etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_properties: Option<std::collections::HashMap<String, String>>,
}

/// Snapshot manager for Iceberg tables
pub struct SnapshotManager {
    metadata_dir: PathBuf,
    current_sequence: i64,
}

impl SnapshotManager {
    /// Create a new snapshot manager
    pub fn new(metadata_dir: impl AsRef<Path>) -> Result<Self> {
        let metadata_dir = metadata_dir.as_ref().to_path_buf();
        
        // Load current sequence number from existing snapshots
        let current_sequence = Self::load_max_sequence(&metadata_dir)?;

        Ok(Self {
            metadata_dir,
            current_sequence,
        })
    }

    /// Create a new snapshot
    pub fn create_snapshot(
        &mut self,
        manifest_list_path: String,
        operation: String,
        schema_id: i32,
    ) -> Result<Snapshot> {
        self.current_sequence += 1;
        let snapshot_id = chrono::Utc::now().timestamp_millis();
        let timestamp_ms = snapshot_id;

        let snapshot = Snapshot {
            sequence_number: self.current_sequence,
            snapshot_id,
            timestamp_ms,
            summary: SnapshotSummary {
                operation,
                additional_properties: None,
            },
            manifest_list: manifest_list_path,
            schema_id,
        };

        // Write snapshot file
        self.write_snapshot(&snapshot)?;

        Ok(snapshot)
    }

    /// Write snapshot to disk
    fn write_snapshot(&self, snapshot: &Snapshot) -> Result<()> {
        let snapshot_file = self.metadata_dir.join(format!("snap-{}.json", snapshot.snapshot_id));
        let json = serde_json::to_string_pretty(snapshot)?;
        fs::write(&snapshot_file, json)?;
        Ok(())
    }

    /// Read snapshot by ID
    pub fn read_snapshot(&self, snapshot_id: i64) -> Result<Snapshot> {
        let snapshot_file = self.metadata_dir.join(format!("snap-{}.json", snapshot_id));
        
        if !snapshot_file.exists() {
            return Err(anyhow::anyhow!("Snapshot {} not found", snapshot_id));
        }

        let json = fs::read_to_string(&snapshot_file)?;
        let snapshot: Snapshot = serde_json::from_str(&json)?;
        Ok(snapshot)
    }

    /// Get latest snapshot
    pub fn get_latest_snapshot(&self) -> Result<Option<Snapshot>> {
        let mut max_snapshot_id: Option<i64> = None;

        // Find the latest snapshot file
        if self.metadata_dir.exists() {
            for entry in fs::read_dir(&self.metadata_dir)? {
                let entry = entry?;
                let file_name = entry.file_name();
                let file_name_str = file_name.to_string_lossy();
                
                if file_name_str.starts_with("snap-") && file_name_str.ends_with(".json") {
                    if let Some(id_str) = file_name_str.strip_prefix("snap-").and_then(|s| s.strip_suffix(".json")) {
                        if let Ok(snapshot_id) = id_str.parse::<i64>() {
                            max_snapshot_id = Some(max_snapshot_id.map_or(snapshot_id, |max| max.max(snapshot_id)));
                        }
                    }
                }
            }
        }

        if let Some(snapshot_id) = max_snapshot_id {
            Ok(Some(self.read_snapshot(snapshot_id)?))
        } else {
            Ok(None)
        }
    }

    /// Load maximum sequence number from existing snapshots
    fn load_max_sequence(metadata_dir: &Path) -> Result<i64> {
        let mut max_sequence = 0i64;

        if !metadata_dir.exists() {
            return Ok(0);
        }

        for entry in fs::read_dir(metadata_dir)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();
            
            if file_name_str.starts_with("snap-") && file_name_str.ends_with(".json") {
                if let Ok(json) = fs::read_to_string(entry.path()) {
                    if let Ok(snapshot) = serde_json::from_str::<Snapshot>(&json) {
                        max_sequence = max_sequence.max(snapshot.sequence_number);
                    }
                }
            }
        }

        Ok(max_sequence)
    }

    /// Get current sequence number
    pub fn current_sequence(&self) -> i64 {
        self.current_sequence
    }
}

