pub mod catalog;
pub mod config;
pub mod manifest;
pub mod parquet_reader;
pub mod parquet_writer;
pub mod schema;
pub mod snapshot;
pub mod write_buffer;

use catalog::IcebergCatalog;
use std::path::PathBuf;

/// Iceberg table manager
pub struct IcebergTable {
    catalog: IcebergCatalog,
    table_name: String,
    base_path: PathBuf,
}

impl IcebergTable {
    pub fn new(catalog: IcebergCatalog, table_name: String, base_path: PathBuf) -> Self {
        Self {
            catalog,
            table_name,
            base_path,
        }
    }

    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    pub fn base_path(&self) -> &PathBuf {
        &self.base_path
    }

    pub fn catalog(&self) -> &IcebergCatalog {
        &self.catalog
    }

    pub fn catalog_mut(&mut self) -> &mut IcebergCatalog {
        &mut self.catalog
    }
}

