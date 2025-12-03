use crate::Value;

/// Projection operator for column selection
pub struct Project {
    // Project state
}

impl Project {
    pub fn new() -> Self {
        Self {}
    }

    pub fn project(&self, _rows: Vec<Vec<Value>>, _column_indices: &[usize]) -> Vec<Vec<Value>> {
        // Projection implementation is handled in executor
        Vec::new()
    }
}

