use crate::Value;

/// Vectorized execution utilities
/// This module provides utilities for batch processing

/// Batch size for vectorized operations
pub const BATCH_SIZE: usize = 1024;

/// Process rows in batches
pub fn process_batches<F>(rows: Vec<Vec<Value>>, mut processor: F)
where
    F: FnMut(&[Vec<Value>]),
{
    for chunk in rows.chunks(BATCH_SIZE) {
        processor(chunk);
    }
}

