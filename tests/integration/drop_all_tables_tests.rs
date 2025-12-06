use vtdb::Database;
use std::path::PathBuf;
use tempfile::TempDir;

/// Create an isolated test database with temporary directory
/// This ensures tests never touch production data in /data/ folder
fn create_test_database() -> (Database, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let iceberg_path = temp_dir.path().join("iceberg");
    std::fs::create_dir_all(&iceberg_path).unwrap();
    
    use vtdb::storage::StorageEngine;
    use vtdb::catalog::Catalog;
    use vtdb::planner::Planner;
    use vtdb::execution::Executor;
    use vtdb::transaction::TransactionManager;
    use vtdb::cache::QueryCache;
    use std::sync::{Arc, Mutex};
    use std::collections::HashMap;
    
    let mut catalog = Catalog::new();
    let mut storage = StorageEngine::with_iceberg_path(&iceberg_path).unwrap();

    // Load tables from storage
    let _ = Database::load_tables_from_storage(&mut catalog, &mut storage);

    let mut db = Database {
        catalog,
        storage,
        planner: Planner::new(),
        executor: Executor::new(),
        transaction_manager: TransactionManager::new(),
        cache: QueryCache::default(),
        active_queries: Arc::new(Mutex::new(HashMap::new())),
        query_results: Arc::new(Mutex::new(HashMap::new())),
    };

    // Clear cache on startup
    db.cache.clear();

    (db, temp_dir)
}

/// Restart test database by creating new Database instance pointing to same test directory
/// This simulates database restart
fn restart_test_database(iceberg_path: &PathBuf) -> Database {
    use vtdb::storage::StorageEngine;
    use vtdb::catalog::Catalog;
    use vtdb::planner::Planner;
    use vtdb::execution::Executor;
    use vtdb::transaction::TransactionManager;
    use vtdb::cache::QueryCache;
    use std::sync::{Arc, Mutex};
    use std::collections::HashMap;
    
    let mut catalog = Catalog::new();
    let mut storage = StorageEngine::with_iceberg_path(iceberg_path).unwrap();

    // Load tables from storage
    let _ = Database::load_tables_from_storage(&mut catalog, &mut storage);

    let mut db = Database {
        catalog,
        storage,
        planner: Planner::new(),
        executor: Executor::new(),
        transaction_manager: TransactionManager::new(),
        cache: QueryCache::default(),
        active_queries: Arc::new(Mutex::new(HashMap::new())),
        query_results: Arc::new(Mutex::new(HashMap::new())),
    };

    // Clear cache on startup
    db.cache.clear();

    db
}

#[test]
fn test_drop_all_tables_when_no_tables_exist() {
    let (mut db, _temp_dir) = create_test_database();

    // Should succeed even when no tables exist
    let result = db.execute("DROP ALL TABLES");
    assert!(result.is_ok(), "DROP ALL TABLES should succeed even with no tables");

    // Verify no tables exist
    let tables = db.list_tables();
    assert_eq!(tables.len(), 0);
}

#[test]
fn test_drop_all_tables_with_single_table() {
    let (mut db, temp_dir) = create_test_database();
    let iceberg_path = temp_dir.path().join("iceberg");

    // Create a table
    db.execute("CREATE TABLE test_table (id INTEGER, name VARCHAR)").unwrap();
    db.execute("INSERT INTO test_table VALUES (1, 'Alice')").unwrap();

    // Verify table exists
    let tables = db.list_tables();
    assert_eq!(tables.len(), 1);
    assert!(tables.contains(&"test_table".to_string()));

    // Verify data exists
    let result = db.execute("SELECT * FROM test_table").unwrap();
    assert_eq!(result.rows.len(), 1);

    // Verify table directory exists in Iceberg storage
    let table_path = iceberg_path.join("test_table");
    assert!(table_path.exists(), "Table directory should exist before DROP ALL TABLES");

    // Drop all tables
    db.execute("DROP ALL TABLES").unwrap();

    // Verify table removed from memory
    let tables = db.list_tables();
    assert_eq!(tables.len(), 0, "Table should be removed from catalog");

    // Verify table directory removed from disk
    assert!(!table_path.exists(), "Table directory should be removed from Iceberg storage");
}

#[test]
fn test_drop_all_tables_with_multiple_tables() {
    let (mut db, temp_dir) = create_test_database();
    let iceberg_path = temp_dir.path().join("iceberg");

    // Create multiple tables
    db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    db.execute("CREATE TABLE orders (id INTEGER, user_id INTEGER)").unwrap();
    db.execute("CREATE TABLE products (id INTEGER, name VARCHAR)").unwrap();

    // Insert data into each
    db.execute("INSERT INTO users VALUES (1, 'Alice')").unwrap();
    db.execute("INSERT INTO orders VALUES (1, 1)").unwrap();
    db.execute("INSERT INTO products VALUES (1, 'Product1')").unwrap();

    // Verify all tables exist
    let tables = db.list_tables();
    assert_eq!(tables.len(), 3);

    // Verify all table directories exist
    assert!(iceberg_path.join("users").exists());
    assert!(iceberg_path.join("orders").exists());
    assert!(iceberg_path.join("products").exists());

    // Drop all tables
    db.execute("DROP ALL TABLES").unwrap();

    // Verify all tables removed from memory
    let tables = db.list_tables();
    assert_eq!(tables.len(), 0);

    // Verify all table directories removed from disk
    assert!(!iceberg_path.join("users").exists());
    assert!(!iceberg_path.join("orders").exists());
    assert!(!iceberg_path.join("products").exists());
}

#[test]
fn test_drop_all_tables_verifies_iceberg_files_deleted() {
    let (mut db, temp_dir) = create_test_database();
    let iceberg_path = temp_dir.path().join("iceberg");

    // Create table and insert data (this creates Iceberg files)
    db.execute("CREATE TABLE test_table (id INTEGER, value INTEGER)").unwrap();
    db.execute("INSERT INTO test_table VALUES (1, 100), (2, 200), (3, 300)").unwrap();

    // Verify Iceberg files exist
    let table_path = iceberg_path.join("test_table");
    let data_dir = table_path.join("data");
    let metadata_dir = table_path.join("metadata");
    
    assert!(table_path.exists());
    assert!(data_dir.exists());
    assert!(metadata_dir.exists());

    // Drop all tables
    db.execute("DROP ALL TABLES").unwrap();

    // Verify all Iceberg files and directories are deleted
    assert!(!table_path.exists(), "Table directory should be completely removed");
    assert!(!data_dir.exists(), "Data directory should be removed");
    assert!(!metadata_dir.exists(), "Metadata directory should be removed");
}

#[test]
fn test_drop_all_tables_clears_cache() {
    let (mut db, _temp_dir) = create_test_database();

    // Create table and insert data
    db.execute("CREATE TABLE cache_test (id INTEGER, value INTEGER)").unwrap();
    db.execute("INSERT INTO cache_test VALUES (1, 100)").unwrap();

    // Run a query to populate cache
    let (result1, from_cache1) = db.execute_with_cache_info("SELECT * FROM cache_test").unwrap();
    assert!(!from_cache1, "First query should not be from cache");
    assert_eq!(result1.rows.len(), 1);

    // Run same query again - should hit cache
    let (result2, from_cache2) = db.execute_with_cache_info("SELECT * FROM cache_test").unwrap();
    assert!(from_cache2, "Second query should be from cache");

    // Drop all tables
    db.execute("DROP ALL TABLES").unwrap();

    // Create new table with same name
    db.execute("CREATE TABLE cache_test (id INTEGER, value INTEGER)").unwrap();
    db.execute("INSERT INTO cache_test VALUES (2, 200)").unwrap();

    // Run query - should not use old cached result
    let (result3, from_cache3) = db.execute_with_cache_info("SELECT * FROM cache_test").unwrap();
    assert!(!from_cache3, "Query after DROP ALL TABLES should not use old cache");
    assert_eq!(result3.rows.len(), 1);
    assert_eq!(result3.rows[0][0], vtdb::Value::Integer(2));
}

#[test]
fn test_drop_all_tables_persists_after_restart() {
    let (mut db, temp_dir) = create_test_database();
    let iceberg_path = temp_dir.path().join("iceberg");

    // Create tables and insert data
    db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    db.execute("CREATE TABLE orders (id INTEGER, user_id INTEGER)").unwrap();
    db.execute("INSERT INTO users VALUES (1, 'Alice')").unwrap();
    db.execute("INSERT INTO orders VALUES (1, 1)").unwrap();

    // Verify tables exist
    assert_eq!(db.list_tables().len(), 2);

    // Drop all tables
    db.execute("DROP ALL TABLES").unwrap();

    // Verify tables removed
    assert_eq!(db.list_tables().len(), 0);

    // Restart database
    let db2 = restart_test_database(&iceberg_path);

    // Verify tables don't reappear after restart
    let tables = db2.list_tables();
    assert_eq!(tables.len(), 0, "Tables should not reappear after restart");

    // Verify table directories still don't exist
    assert!(!iceberg_path.join("users").exists());
    assert!(!iceberg_path.join("orders").exists());
}

#[test]
fn test_drop_all_tables_after_inserting_data() {
    let (mut db, temp_dir) = create_test_database();
    let iceberg_path = temp_dir.path().join("iceberg");

    // Create table and insert substantial data
    db.execute("CREATE TABLE large_table (id INTEGER, data VARCHAR)").unwrap();
    
    // Insert multiple rows
    for i in 1..=100 {
        db.execute(&format!("INSERT INTO large_table VALUES ({}, 'data_{}')", i, i)).unwrap();
    }

    // Verify data exists
    let result = db.execute("SELECT COUNT(*) FROM large_table").unwrap();
    assert_eq!(result.rows[0][0], vtdb::Value::Integer(100));

    // Verify table directory exists with data files
    let table_path = iceberg_path.join("large_table");
    assert!(table_path.exists());
    let data_dir = table_path.join("data");
    assert!(data_dir.exists());

    // Drop all tables
    db.execute("DROP ALL TABLES").unwrap();

    // Verify table and all data are gone
    assert!(!table_path.exists(), "Table directory should be removed");
    
    // Verify can't query the table
    let query_result = db.execute("SELECT * FROM large_table");
    assert!(query_result.is_err(), "Querying dropped table should fail");
}

#[test]
fn test_drop_all_tables_allows_creating_new_tables() {
    let (mut db, _temp_dir) = create_test_database();

    // Create and drop tables
    db.execute("CREATE TABLE old_table (id INTEGER)").unwrap();
    db.execute("INSERT INTO old_table VALUES (1)").unwrap();
    db.execute("DROP ALL TABLES").unwrap();

    // Verify old table is gone
    assert_eq!(db.list_tables().len(), 0);

    // Create new tables with same or different names
    db.execute("CREATE TABLE new_table (id INTEGER, name VARCHAR)").unwrap();
    db.execute("CREATE TABLE old_table (id INTEGER)").unwrap(); // Can reuse name

    // Verify new tables exist
    let tables = db.list_tables();
    assert_eq!(tables.len(), 2);
    assert!(tables.contains(&"new_table".to_string()));
    assert!(tables.contains(&"old_table".to_string()));

    // Verify can insert and query new tables
    db.execute("INSERT INTO new_table VALUES (1, 'Test')").unwrap();
    let result = db.execute("SELECT * FROM new_table").unwrap();
    assert_eq!(result.rows.len(), 1);
}

