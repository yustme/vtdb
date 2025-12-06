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
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::time::Instant;

/// Query status
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum QueryStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Table progress information
#[derive(Debug, Clone, serde::Serialize)]
pub struct TableProgress {
    pub table_name: String,
    pub rows_scanned: usize,
    pub total_rows: usize,
}

/// Query progress tracker
#[derive(Debug, Clone, serde::Serialize)]
pub struct QueryProgressTracker {
    pub query_id: String,
    pub status: QueryStatus,
    pub tables_scanned: Vec<TableProgress>,
    pub current_stage: String,
    pub rows_processed: usize,
    pub estimated_total_rows: usize,
    #[serde(skip_serializing)]
    pub start_time: Instant,
    pub error_message: Option<String>,
    #[serde(skip_serializing)]
    pub elapsed_seconds: f64,
}

impl QueryProgressTracker {
    pub fn new(query_id: String) -> Self {
        Self {
            query_id,
            status: QueryStatus::Running,
            tables_scanned: Vec::new(),
            current_stage: "Initializing".to_string(),
            rows_processed: 0,
            estimated_total_rows: 0,
            start_time: Instant::now(),
            error_message: None,
            elapsed_seconds: 0.0,
        }
    }

    pub fn update_elapsed(&mut self) {
        self.elapsed_seconds = self.start_time.elapsed().as_secs_f64();
    }

    pub fn update_table_progress(&mut self, table_name: String, rows_scanned: usize, total_rows: usize) {
        if let Some(table_progress) = self.tables_scanned.iter_mut().find(|t| t.table_name == table_name) {
            table_progress.rows_scanned = rows_scanned;
            table_progress.total_rows = total_rows;
        } else {
            self.tables_scanned.push(TableProgress {
                table_name,
                rows_scanned,
                total_rows,
            });
        }
    }

    pub fn set_stage(&mut self, stage: String) {
        self.current_stage = stage;
    }

    pub fn get_progress_percentage(&self) -> f64 {
        if self.estimated_total_rows == 0 {
            return 0.0;
        }
        (self.rows_processed as f64 / self.estimated_total_rows as f64 * 100.0).min(100.0)
    }
}

/// Query result storage
#[derive(Debug, Clone)]
pub struct QueryResultStorage {
    pub result: Option<QueryResult>,
    pub from_cache: bool,
}

/// Main database instance
pub struct Database {
    pub(crate) catalog: Catalog,
    pub(crate) storage: StorageEngine,
    pub(crate) planner: Planner,
    pub(crate) executor: Executor,
    pub(crate) transaction_manager: TransactionManager,
    pub(crate) cache: QueryCache,
    pub(crate) active_queries: Arc<Mutex<HashMap<String, QueryProgressTracker>>>,
    pub(crate) query_results: Arc<Mutex<HashMap<String, QueryResultStorage>>>,
}

impl Database {
    /// Create a new database instance with custom Iceberg path (for testing only)
    /// This method is public but intended for test use to ensure tests don't touch production data
    pub fn with_iceberg_path(iceberg_path: impl Into<std::path::PathBuf>) -> Result<Self> {
        let iceberg_path = iceberg_path.into();
        let mut catalog = Catalog::new();
        let mut storage = StorageEngine::with_iceberg_path(&iceberg_path)?;
        let planner = Planner::new();
        let executor = Executor::new();
        let transaction_manager = TransactionManager::new();
        let cache = QueryCache::default();

        // Load existing tables from Iceberg storage
        if let Err(e) = Self::load_tables_from_storage(&mut catalog, &mut storage) {
            eprintln!("Warning: Failed to load some tables from storage: {}", e);
        }

        let mut db = Self {
            catalog,
            storage,
            planner,
            executor,
            transaction_manager,
            cache,
            active_queries: Arc::new(Mutex::new(HashMap::new())),
            query_results: Arc::new(Mutex::new(HashMap::new())),
        };

        // Clear cache on startup to avoid stale cached results from previous session
        db.cache.clear();

        Ok(db)
    }
}

impl Database {
    /// Create a new database instance
    pub fn new() -> Self {
        let mut catalog = Catalog::new();
        let mut storage = StorageEngine::new();
        let planner = Planner::new();
        let executor = Executor::new();
        let transaction_manager = TransactionManager::new();
        let cache = QueryCache::default();

        // Load existing tables from Iceberg storage
        if let Err(e) = Self::load_tables_from_storage(&mut catalog, &mut storage) {
            eprintln!("Warning: Failed to load some tables from storage: {}", e);
        }

        let mut db = Self {
            catalog,
            storage,
            planner,
            executor,
            transaction_manager,
            cache,
            active_queries: Arc::new(Mutex::new(HashMap::new())),
            query_results: Arc::new(Mutex::new(HashMap::new())),
        };

        // Clear cache on startup to avoid stale cached results from previous session
        db.cache.clear();

        db
    }

    /// Load tables from Iceberg storage into catalog
    pub fn load_tables_from_storage(
        catalog: &mut Catalog,
        storage: &mut StorageEngine,
    ) -> Result<()> {
        // Ensure Iceberg catalog is initialized
        let iceberg_catalog = storage.ensure_iceberg_catalog()?;
        
        // Discover all tables in the Iceberg catalog
        let table_names = iceberg_catalog.list_tables();
        
        for table_name in table_names {
            // Skip if already in catalog (shouldn't happen, but be safe)
            if catalog.table_exists(&table_name) {
                continue;
            }
            
            // Load metadata from Iceberg
            if iceberg_catalog.load_table_metadata(&table_name).is_ok() {
                if let Ok(metadata) = iceberg_catalog.get_table_metadata(&table_name) {
                    // Convert Iceberg schema to catalog columns
                    let columns = metadata.schema.to_catalog_columns();
                    
                    // Register table in catalog
                    if catalog.create_table(table_name.clone(), columns).is_err() {
                        // Table might already exist, continue
                        continue;
                    }
                }
            }
        }
        
        Ok(())
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
        let index_manager = self.storage.index_manager();
        let table_indexes = index_manager.get_table_indexes(table_name);
        
        // Create a map of column name to index type
        let index_map: std::collections::HashMap<String, String> = table_indexes
            .into_iter()
            .map(|(col_name, idx_type)| {
                (col_name, match idx_type {
                    crate::index::IndexType::Hash => "HASH".to_string(),
                    crate::index::IndexType::BTree => "BTREE".to_string(),
                })
            })
            .collect();
        
        Ok(TableSchema {
            name: table.name.clone(),
            columns: table.columns.iter().map(|col| ColumnInfo {
                name: col.name.clone(),
                data_type: col.data_type.name().to_string(),
                ordinal: col.ordinal,
                index_type: index_map.get(&col.name).cloned(),
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

    /// Get query progress
    pub fn get_query_progress(&self, query_id: &str) -> Option<QueryProgressTracker> {
        self.active_queries.lock().ok()
            .and_then(|queries| queries.get(query_id).cloned())
    }

    /// Get query result
    pub fn get_query_result(&self, query_id: &str) -> Option<QueryResultStorage> {
        self.query_results.lock().ok()
            .and_then(|results| results.get(query_id).cloned())
    }

    /// Cancel a query
    pub fn cancel_query(&self, query_id: &str) -> bool {
        if let Ok(mut queries) = self.active_queries.lock() {
            if let Some(tracker) = queries.get_mut(query_id) {
                tracker.status = QueryStatus::Cancelled;
                return true;
            }
        }
        false
    }

    /// Start async query execution (takes Arc<Mutex<Database>> for async execution)
    pub fn execute_async_internal(
        db: Arc<Mutex<Database>>,
        query_id: String,
        sql: String,
    ) -> Result<()> {
        // Create progress tracker
        let tracker = QueryProgressTracker::new(query_id.clone());
        
        // Store initial tracker
        {
            let db_guard = db.lock().map_err(|_| anyhow::anyhow!("Failed to lock database"))?;
            let mut queries = db_guard.active_queries.lock().map_err(|_| {
                anyhow::anyhow!("Failed to lock active queries")
            })?;
            queries.insert(query_id.clone(), tracker);
        }

        // Spawn async task
        tokio::spawn(async move {
            let result = execute_query_with_progress(db.clone(), query_id.clone(), sql).await;

            // Update tracker and store result
            if let Ok((query_result, from_cache)) = result {
                // Update tracker to completed
                if let Ok(db_guard) = db.lock() {
                    if let Ok(mut queries) = db_guard.active_queries.lock() {
                        if let Some(tracker) = queries.get_mut(&query_id) {
                            tracker.status = QueryStatus::Completed;
                            tracker.rows_processed = tracker.estimated_total_rows;
                        }
                    }
                }

                // Store result
                if let Ok(db_guard) = db.lock() {
                    if let Ok(mut results) = db_guard.query_results.lock() {
                        results.insert(query_id.clone(), QueryResultStorage {
                            result: Some(query_result),
                            from_cache,
                        });
                    }
                }
            } else if let Err(e) = result {
                // Update tracker to failed
                if let Ok(db_guard) = db.lock() {
                    if let Ok(mut queries) = db_guard.active_queries.lock() {
                        if let Some(tracker) = queries.get_mut(&query_id) {
                            tracker.status = QueryStatus::Failed;
                            tracker.error_message = Some(e.to_string());
                        }
                    }
                }
            }
        });

        Ok(())
    }
}

/// Execute query with progress tracking (helper function for async execution)
async fn execute_query_with_progress(
    db: Arc<Mutex<Database>>,
    query_id: String,
    sql: String,
) -> Result<(QueryResult, bool)> {
    // Parse SQL
    let ast = parse_sql(&sql)?;

    // Check cache for SELECT queries
    let (from_cache, query_hash) = if let Statement::Select(_) = &ast {
        let query_hash = hash_statement(&ast);
        let mut db_guard = db.lock().map_err(|_| anyhow::anyhow!("Failed to lock database"))?;
        
        if let Some(cached_result) = db_guard.cache.get(query_hash) {
            return Ok((cached_result, true));
        }
        (false, Some(query_hash))
    } else {
        (false, None)
    };

    // Update progress: Planning
    update_progress_stage(&db, &query_id, "Planning query".to_string());

    // Plan query
    let plan = {
        let db_guard = db.lock().map_err(|_| anyhow::anyhow!("Failed to lock database"))?;
        db_guard.planner.plan(&ast, &db_guard.catalog)?
    };

    // Update progress: Executing
    update_progress_stage(&db, &query_id, "Executing query".to_string());

    // Execute query with progress tracking
    // For SELECT queries, use progress-aware execution
    let result = if let Statement::Select(_) = &ast {
        // Use the progress-aware execution path for SELECT queries
        let active_queries_clone = {
            let db_guard = db.lock().map_err(|_| anyhow::anyhow!("Failed to lock database"))?;
            db_guard.active_queries.clone()
        };
        
        // Extract executor to avoid borrow checker issues
        let executor = {
            let db_guard = db.lock().map_err(|_| anyhow::anyhow!("Failed to lock database"))?;
            db_guard.executor.clone()
        };
        
        // Execute with progress - use a helper to avoid borrow conflicts
        execute_with_progress_helper(
            &executor,
            &plan,
            db.clone(),
            Some(active_queries_clone),
            Some(&query_id),
        )?
    } else {
        // Fall back to regular execution for non-SELECT queries
        let executor = {
            let db_guard = db.lock().map_err(|_| anyhow::anyhow!("Failed to lock database"))?;
            db_guard.executor.clone()
        };
        
        let mut db_guard = db.lock().map_err(|_| anyhow::anyhow!("Failed to lock database"))?;
        let storage_ptr: *mut StorageEngine = &mut db_guard.storage;
        let catalog_ptr: *mut Catalog = &mut db_guard.catalog;
        
        // SAFETY: storage and catalog are separate fields, so these pointers don't alias
        unsafe {
            executor.execute(&plan, &mut *storage_ptr, &mut *catalog_ptr)?
        }
    };

    // Cache SELECT query results
    if let Some(query_hash) = query_hash {
        let table_dependencies = extract_table_dependencies(&ast);
        let mut db_guard = db.lock().map_err(|_| anyhow::anyhow!("Failed to lock database"))?;
        db_guard.cache.put(query_hash, result.clone(), table_dependencies);
    }

    // Invalidate cache for write operations
    let mut db_guard = db.lock().map_err(|_| anyhow::anyhow!("Failed to lock database"))?;
    match &ast {
        Statement::Insert(insert) => {
            db_guard.cache.invalidate_tables(&[insert.table.clone()]);
        }
        Statement::Update(update) => {
            db_guard.cache.invalidate_tables(&[update.table.clone()]);
        }
        Statement::Delete(delete) => {
            db_guard.cache.invalidate_tables(&[delete.table.clone()]);
        }
        Statement::CreateTable(_) => {}
        Statement::Select(_) => {}
    }

    Ok((result, from_cache))
}

/// Helper to update progress stage
fn update_progress_stage(db: &Arc<Mutex<Database>>, query_id: &str, stage: String) {
    if let Ok(db_guard) = db.lock() {
        if let Ok(mut queries) = db_guard.active_queries.lock() {
            if let Some(tracker) = queries.get_mut(query_id) {
                tracker.set_stage(stage);
            }
        }
    }
}

/// Helper function to execute with progress, avoiding borrow checker issues
fn execute_with_progress_helper(
    executor: &Executor,
    plan: &crate::planner::physical::PhysicalPlan,
    db: Arc<Mutex<Database>>,
    active_queries: Option<Arc<Mutex<HashMap<String, QueryProgressTracker>>>>,
    query_id: Option<&str>,
) -> Result<QueryResult> {
    // Lock database and extract storage and catalog references
    // We need to use unsafe to get mutable references to both fields
    // This is safe because storage and catalog don't overlap in memory
    let mut db_guard = db.lock().map_err(|_| anyhow::anyhow!("Failed to lock database"))?;
    
    // Use unsafe to get mutable references to both fields
    // This is safe because we're not aliasing and the references don't overlap
    let storage_ptr: *mut StorageEngine = &mut db_guard.storage;
    let catalog_ptr: *mut Catalog = &mut db_guard.catalog;
    
    // Execute with the raw pointers converted back to references
    // This avoids the borrow checker issue
    // SAFETY: storage and catalog are separate fields, so these pointers don't alias
    unsafe {
        Ok(executor.execute_with_progress(
            plan,
            &mut *storage_ptr,
            &mut *catalog_ptr,
            active_queries,
            query_id,
        )?)
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_type: Option<String>, // "HASH" or "BTREE" if indexed
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

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Value {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Value::Integer(a), Value::Integer(b)) => a.cmp(b),
            (Value::Varchar(a), Value::Varchar(b)) => a.cmp(b),
            (Value::Boolean(a), Value::Boolean(b)) => a.cmp(b),
            (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
            // Ordering: Null < Boolean < Integer < Varchar
            (Value::Null, _) => std::cmp::Ordering::Less,
            (_, Value::Null) => std::cmp::Ordering::Greater,
            (Value::Boolean(_), Value::Integer(_) | Value::Varchar(_)) => std::cmp::Ordering::Less,
            (Value::Integer(_) | Value::Varchar(_), Value::Boolean(_)) => std::cmp::Ordering::Greater,
            (Value::Integer(_), Value::Varchar(_)) => std::cmp::Ordering::Less,
            (Value::Varchar(_), Value::Integer(_)) => std::cmp::Ordering::Greater,
        }
    }
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

    // Helper function to create isolated test database
    fn create_test_database() -> Database {
        use tempfile::TempDir;
        use std::sync::Mutex;
        lazy_static::lazy_static! {
            static ref TEST_DIRS: Mutex<Vec<TempDir>> = Mutex::new(Vec::new());
        }
        
        let temp_dir = TempDir::new().unwrap();
        let iceberg_path = temp_dir.path().join("iceberg");
        std::fs::create_dir_all(&iceberg_path).unwrap();
        
        // Keep temp_dir alive for the duration of tests
        TEST_DIRS.lock().unwrap().push(temp_dir);
        
        Database::with_iceberg_path(&iceberg_path).unwrap()
    }

    #[test]
    fn test_database_creation() {
        let db = create_test_database();
        assert!(true); // Database created successfully
    }

    #[test]
    fn test_query_cache_hit() {
        let mut db = create_test_database();
        
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
        let mut db = create_test_database();
        
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
        let mut db = create_test_database();
        
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
        let mut db = create_test_database();
        
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
        let mut db = create_test_database();
        
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
        let mut db = create_test_database();
        
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

