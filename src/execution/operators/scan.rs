use crate::Value;

/// Table scan operator
pub struct TableScan {
    // Scan state
}

impl TableScan {
    pub fn new() -> Self {
        Self {}
    }

    pub fn scan(&self, _table_name: &str) -> Vec<Vec<Value>> {
        // Scan implementation is handled by storage engine
        Vec::new()
    }
}

