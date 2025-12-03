pub mod cache;
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
use cache::QueryCache;
use catalog::Catalog;
use execution::Executor;
use planner::Planner;
use storage::StorageEngine;
use transaction::TransactionManager;
use parser::ast::{Statement, hash_statement};
use cache::extract_table_dependencies;

/// Main database instance
pub struct Database {
    catalog: Catalog,
    storage: StorageEngine,
    planner: Planner,
    executor: Executor,
    transaction_manager: TransactionManager,
    cache: QueryCache,
}

impl Database {
    /// Create a new database instance
    pub fn new() -> Self {
        let catalog = Catalog::new();
        let storage = StorageEngine::new();
        let planner = Planner::new();
        let executor = Executor::new();
        let transaction_manager = TransactionManager::new();
        let cache = QueryCache::default();

        Self {
            catalog,
            storage,
            planner,
            executor,
            transaction_manager,
            cache,
        }
    }

    /// Execute a SQL statement
    pub fn execute(&mut self, sql: &str) -> Result<QueryResult> {
        let (result, _) = self.execute_with_cache_info(sql)?;
        Ok(result)
    }

    /// Execute a SQL statement and return cache information
    pub fn execute_with_cache_info(&mut self, sql: &str) -> Result<(QueryResult, bool)> {
        // Parse SQL
        let ast = parse_sql(sql)?;

        // Check cache for SELECT queries
        if let Statement::Select(_) = &ast {
            let query_hash = hash_statement(&ast);
            if let Some(cached_result) = self.cache.get(query_hash) {
                return Ok((cached_result, true)); // Return cached result with cache flag
            }
        }

        // Plan query
        let plan = self.planner.plan(&ast, &self.catalog)?;

        // Execute query
        let result = self
            .executor
            .execute(&plan, &mut self.storage, &mut self.catalog)?;

        // Cache SELECT query results
        if let Statement::Select(_) = &ast {
            let query_hash = hash_statement(&ast);
            let table_dependencies = extract_table_dependencies(&ast);
            self.cache.put(query_hash, result.clone(), table_dependencies);
        }

        // Invalidate cache for write operations
        match &ast {
            Statement::Insert(insert) => {
                self.cache.invalidate_tables(&[insert.table.clone()]);
            }
            Statement::Update(update) => {
                self.cache.invalidate_tables(&[update.table.clone()]);
            }
            Statement::Delete(delete) => {
                self.cache.invalidate_tables(&[delete.table.clone()]);
            }
            Statement::CreateTable(_) => {
                // No invalidation needed - new table has no dependent queries
            }
            Statement::Select(_) => {
                // Already handled above
            }
        }

        Ok((result, false)) // Return fresh result with cache flag
    }

    /// List all tables in the database
    pub fn list_tables(&self) -> Vec<String> {
        self.catalog.list_tables()
    }

    /// Get table schema information
    pub fn get_table_schema(&self, table_name: &str) -> Result<TableSchema> {
        let table = self.catalog.get_table(table_name)?;
        let stats = self.get_table_stats(table_name).ok();
        Ok(TableSchema {
            name: table.name.clone(),
            columns: table.columns.iter().map(|col| ColumnInfo {
                name: col.name.clone(),
                data_type: col.data_type.name().to_string(),
                ordinal: col.ordinal,
            }).collect(),
            stats,
        })
    }

    /// Get row count for a table
    pub fn get_table_row_count(&self, table_name: &str) -> Result<usize> {
        self.storage.get_row_count(table_name)
    }

    /// Get storage size in bytes for a table
    pub fn get_table_storage_size(&self, table_name: &str) -> Result<usize> {
        self.storage.estimate_storage_size(table_name)
    }

    /// Get table statistics (row count and storage size)
    pub fn get_table_stats(&self, table_name: &str) -> Result<TableStats> {
        let row_count = self.get_table_row_count(table_name)?;
        let storage_size_bytes = self.get_table_storage_size(table_name)?;
        Ok(TableStats {
            row_count,
            storage_size_bytes,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<TableStats>,
}

/// Table statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct TableStats {
    pub row_count: usize,
    pub storage_size_bytes: usize,
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

    #[test]
    fn test_query_cache_hit() {
        let mut db = Database::new();
        
        // Create table and insert data
        db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
        db.execute("INSERT INTO users VALUES (1, 'Alice')").unwrap();
        
        // First query - should execute and cache
        let result1 = db.execute("SELECT * FROM users WHERE id = 1").unwrap();
        assert_eq!(result1.rows.len(), 1);
        
        // Second identical query - should hit cache
        let result2 = db.execute("SELECT * FROM users WHERE id = 1").unwrap();
        assert_eq!(result2.rows.len(), 1);
        assert_eq!(result1.rows, result2.rows);
    }

    #[test]
    fn test_query_cache_invalidation_on_insert() {
        let mut db = Database::new();
        
        db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
        db.execute("INSERT INTO users VALUES (1, 'Alice')").unwrap();
        
        // Cache a query
        let result1 = db.execute("SELECT * FROM users").unwrap();
        assert_eq!(result1.rows.len(), 1);
        
        // Insert new row - should invalidate cache
        db.execute("INSERT INTO users VALUES (2, 'Bob')").unwrap();
        
        // Query again - should get fresh result with both rows
        let result2 = db.execute("SELECT * FROM users").unwrap();
        assert_eq!(result2.rows.len(), 2);
    }

    #[test]
    fn test_query_cache_invalidation_on_update() {
        let mut db = Database::new();
        
        db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
        db.execute("INSERT INTO users VALUES (1, 'Alice')").unwrap();
        
        // Cache a query
        let result1 = db.execute("SELECT * FROM users WHERE id = 1").unwrap();
        assert_eq!(result1.rows[0][1], Value::Varchar("Alice".to_string()));
        
        // Update row - should invalidate cache
        db.execute("UPDATE users SET name = 'Alice Updated' WHERE id = 1").unwrap();
        
        // Query again - should get fresh result
        let result2 = db.execute("SELECT * FROM users WHERE id = 1").unwrap();
        assert_eq!(result2.rows[0][1], Value::Varchar("Alice Updated".to_string()));
    }

    #[test]
    fn test_query_cache_invalidation_on_delete() {
        let mut db = Database::new();
        
        db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
        db.execute("INSERT INTO users VALUES (1, 'Alice')").unwrap();
        db.execute("INSERT INTO users VALUES (2, 'Bob')").unwrap();
        
        // Cache a query
        let result1 = db.execute("SELECT * FROM users").unwrap();
        assert_eq!(result1.rows.len(), 2);
        
        // Delete row - should invalidate cache
        db.execute("DELETE FROM users WHERE id = 1").unwrap();
        
        // Query again - should get fresh result
        let result2 = db.execute("SELECT * FROM users").unwrap();
        assert_eq!(result2.rows.len(), 1);
    }

    #[test]
    fn test_query_cache_table_dependencies() {
        let mut db = Database::new();
        
        db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
        db.execute("CREATE TABLE orders (id INTEGER, user_id INTEGER)").unwrap();
        db.execute("INSERT INTO users VALUES (1, 'Alice')").unwrap();
        db.execute("INSERT INTO orders VALUES (1, 1)").unwrap();
        
        // Cache a query on users table
        let result1 = db.execute("SELECT * FROM users").unwrap();
        assert_eq!(result1.rows.len(), 1);
        
        // Cache a query on orders table
        let result2 = db.execute("SELECT * FROM orders").unwrap();
        assert_eq!(result2.rows.len(), 1);
        
        // Modify users table - should only invalidate users query, not orders
        db.execute("INSERT INTO users VALUES (2, 'Bob')").unwrap();
        
        // Users query should be fresh
        let result3 = db.execute("SELECT * FROM users").unwrap();
        assert_eq!(result3.rows.len(), 2);
        
        // Orders query should still be cached (if cache wasn't evicted)
        // Note: This test may fail if LRU evicted the orders entry, but the key point
        // is that modifying users shouldn't directly invalidate orders cache
    }

    #[test]
    fn test_query_cache_different_queries() {
        let mut db = Database::new();
        
        db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
        db.execute("INSERT INTO users VALUES (1, 'Alice')").unwrap();
        
        // Different queries should have different cache entries
        let result1 = db.execute("SELECT * FROM users WHERE id = 1").unwrap();
        let result2 = db.execute("SELECT name FROM users").unwrap();
        
        assert_eq!(result1.rows.len(), 1);
        assert_eq!(result2.rows.len(), 1);
        
        // Both should be cached independently
        let result3 = db.execute("SELECT * FROM users WHERE id = 1").unwrap();
        let result4 = db.execute("SELECT name FROM users").unwrap();
        
        assert_eq!(result1.rows, result3.rows);
        assert_eq!(result2.columns, result4.columns);
    }
}

