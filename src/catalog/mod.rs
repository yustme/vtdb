pub mod schema;
pub mod types;

pub use schema::{Schema, Table};
pub use types::DataType;

use anyhow::{anyhow, Result};
use std::collections::HashMap;

/// Catalog manager for schema metadata
pub struct Catalog {
    schemas: HashMap<String, Schema>,
}

impl Catalog {
    pub fn new() -> Self {
        Self {
            schemas: HashMap::new(),
        }
    }

    /// Create a new table
    pub fn create_table(&mut self, name: String, columns: Vec<(String, DataType)>) -> Result<()> {
        let schema_name = "PUBLIC".to_string(); // Default schema
        let schema = self.schemas.entry(schema_name.clone()).or_insert_with(|| {
            Schema::new(schema_name)
        });

        if schema.has_table(&name) {
            return Err(anyhow!("Table '{}' already exists", name));
        }

        let table = Table::new(name.clone(), columns);
        schema.add_table(name, table);
        Ok(())
    }

    /// Get table schema
    pub fn get_table(&self, name: &str) -> Result<&Table> {
        let schema = self.schemas.get("PUBLIC").ok_or_else(|| {
            anyhow!("Schema 'PUBLIC' not found")
        })?;

        schema.get_table(name).ok_or_else(|| {
            anyhow!("Table '{}' not found", name)
        })
    }

    /// Check if table exists
    pub fn table_exists(&self, name: &str) -> bool {
        self.schemas
            .get("PUBLIC")
            .map(|s| s.has_table(name))
            .unwrap_or(false)
    }
}

impl Default for Catalog {
    fn default() -> Self {
        Self::new()
    }
}

