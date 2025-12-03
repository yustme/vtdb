pub mod catalog;
pub mod execution;
pub mod index;
pub mod parser;
pub mod planner;
pub mod storage;
pub mod transaction;
pub mod web;

// Re-export commonly used types
pub use parser::parse_sql;

use anyhow::Result;
use catalog::Catalog;
use execution::Executor;
use planner::Planner;
use storage::StorageEngine;
use transaction::TransactionManager;

/// Main database instance
pub struct Database {
    catalog: Catalog,
    storage: StorageEngine,
    planner: Planner,
    executor: Executor,
    transaction_manager: TransactionManager,
}

impl Database {
    /// Create a new database instance
    pub fn new() -> Self {
        let catalog = Catalog::new();
        let storage = StorageEngine::new();
        let planner = Planner::new();
        let executor = Executor::new();
        let transaction_manager = TransactionManager::new();

        Self {
            catalog,
            storage,
            planner,
            executor,
            transaction_manager,
        }
    }

    /// Execute a SQL statement
    pub fn execute(&mut self, sql: &str) -> Result<QueryResult> {
        // Parse SQL
        let ast = parse_sql(sql)?;

        // Plan query
        let plan = self.planner.plan(&ast, &self.catalog)?;

        // Execute query
        let result = self
            .executor
            .execute(&plan, &mut self.storage, &mut self.catalog)?;

        Ok(result)
    }

    /// List all tables in the database
    pub fn list_tables(&self) -> Vec<String> {
        self.catalog.list_tables()
    }

    /// Get table schema information
    pub fn get_table_schema(&self, table_name: &str) -> Result<TableSchema> {
        let table = self.catalog.get_table(table_name)?;
        Ok(TableSchema {
            name: table.name.clone(),
            columns: table.columns.iter().map(|col| ColumnInfo {
                name: col.name.clone(),
                data_type: col.data_type.name().to_string(),
                ordinal: col.ordinal,
            }).collect(),
        })
    }
}

impl Default for Database {
    fn default() -> Self {
        Self::new()
    }
}

/// Query execution result
#[derive(Debug, Clone, serde::Serialize)]
pub struct QueryResult {
    pub rows: Vec<Vec<Value>>,
    pub columns: Vec<String>,
}

/// Table schema information
#[derive(Debug, Clone, serde::Serialize)]
pub struct TableSchema {
    pub name: String,
    pub columns: Vec<ColumnInfo>,
}

/// Column information
#[derive(Debug, Clone, serde::Serialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub ordinal: usize,
}

/// Database value types
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(tag = "type", content = "value")]
pub enum Value {
    Integer(i64),
    Varchar(String),
    Boolean(bool),
    Null,
}

impl Value {
    pub fn to_string(&self) -> String {
        match self {
            Value::Integer(i) => i.to_string(),
            Value::Varchar(s) => s.clone(),
            Value::Boolean(b) => b.to_string(),
            Value::Null => "NULL".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_creation() {
        let db = Database::new();
        assert!(true); // Database created successfully
    }
}

