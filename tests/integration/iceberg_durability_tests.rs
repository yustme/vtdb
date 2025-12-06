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
fn test_basic_table_creation_and_persistence() {
    let (mut db, temp_dir) = create_test_database();
    let iceberg_path = temp_dir.path().join("iceberg");

    // Create table with multiple columns
    db.execute("CREATE TABLE test_table (id INTEGER, name VARCHAR, active BOOLEAN)").unwrap();

    // Insert multiple rows
    db.execute("INSERT INTO test_table VALUES (1, 'Alice', true)").unwrap();
    db.execute("INSERT INTO test_table VALUES (2, 'Bob', false)").unwrap();
    db.execute("INSERT INTO test_table VALUES (3, 'Charlie', true)").unwrap();

    // Verify data before restart
    let result = db.execute("SELECT * FROM test_table ORDER BY id").unwrap();
    assert_eq!(result.rows.len(), 3);
    assert_eq!(result.columns, vec!["id", "name", "active"]);

    // Restart database
    let mut db2 = restart_test_database(&iceberg_path);

    // Verify table exists and has correct schema
    let tables = db2.list_tables();
    assert!(tables.contains(&"test_table".to_string()));

    // Verify all rows are present
    let result2 = db2.execute("SELECT * FROM test_table ORDER BY id").unwrap();
    assert_eq!(result2.rows.len(), 3);
    assert_eq!(result2.columns, vec!["id", "name", "active"]);
    
    // Verify data integrity
    assert_eq!(result2.rows[0][0], vtdb::Value::Integer(1));
    assert_eq!(result2.rows[1][0], vtdb::Value::Integer(2));
    assert_eq!(result2.rows[2][0], vtdb::Value::Integer(3));
}

#[test]
fn test_multiple_tables_with_various_data_types() {
    let (mut db, temp_dir) = create_test_database();
    let iceberg_path = temp_dir.path().join("iceberg");

    // Create multiple tables with different schemas
    db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    db.execute("CREATE TABLE orders (order_id INTEGER, customer_id INTEGER, total INTEGER)").unwrap();
    db.execute("CREATE TABLE flags (id INTEGER, enabled BOOLEAN)").unwrap();

    // Insert data into each table
    db.execute("INSERT INTO users VALUES (1, 'Alice'), (2, 'Bob')").unwrap();
    db.execute("INSERT INTO orders VALUES (101, 1, 100), (102, 2, 200)").unwrap();
    db.execute("INSERT INTO flags VALUES (1, true), (2, false)").unwrap();

    // Restart database
    let mut db2 = restart_test_database(&iceberg_path);

    // Verify all tables exist with correct schemas
    let tables = db2.list_tables();
    assert!(tables.contains(&"users".to_string()));
    assert!(tables.contains(&"orders".to_string()));
    assert!(tables.contains(&"flags".to_string()));

    // Verify row counts match expected values
    assert_eq!(db2.get_table_row_count("users").unwrap(), 2);
    assert_eq!(db2.get_table_row_count("orders").unwrap(), 2);
    assert_eq!(db2.get_table_row_count("flags").unwrap(), 2);

    // Query each table and verify data correctness
    let users_result = db2.execute("SELECT * FROM users ORDER BY id").unwrap();
    assert_eq!(users_result.rows.len(), 2);
    
    let orders_result = db2.execute("SELECT * FROM orders ORDER BY order_id").unwrap();
    assert_eq!(orders_result.rows.len(), 2);
    
    let flags_result = db2.execute("SELECT * FROM flags ORDER BY id").unwrap();
    assert_eq!(flags_result.rows.len(), 2);
    assert_eq!(flags_result.rows[0][1], vtdb::Value::Boolean(true));
    assert_eq!(flags_result.rows[1][1], vtdb::Value::Boolean(false));
}

#[test]
fn test_incremental_inserts_across_restarts() {
    let (mut db, temp_dir) = create_test_database();
    let iceberg_path = temp_dir.path().join("iceberg");

    // Create table
    db.execute("CREATE TABLE items (id INTEGER, value INTEGER)").unwrap();

    // Insert batch 1
    let batch1: Vec<String> = (1..=1000).map(|i| format!("({}, {})", i, i * 10)).collect();
    let values = batch1.join(", ");
    db.execute(&format!("INSERT INTO items VALUES {}", values)).unwrap();

    // Restart database
    let mut db2 = restart_test_database(&iceberg_path);

    // Verify batch 1 exists
    let count1 = db2.execute("SELECT COUNT(*) FROM items").unwrap();
    assert_eq!(count1.rows[0][0], vtdb::Value::Integer(1000));

    // Insert batch 2
    let batch2: Vec<String> = (1001..=1500).map(|i| format!("({}, {})", i, i * 10)).collect();
    let values2 = batch2.join(", ");
    db2.execute(&format!("INSERT INTO items VALUES {}", values2)).unwrap();

    // Restart database again
    let mut db3 = restart_test_database(&iceberg_path);

    // Verify both batches exist (total 1500 rows)
    let count2 = db3.execute("SELECT COUNT(*) FROM items").unwrap();
    assert_eq!(count2.rows[0][0], vtdb::Value::Integer(1500));

    // Insert batch 3
    let batch3: Vec<String> = (1501..=2000).map(|i| format!("({}, {})", i, i * 10)).collect();
    let values3 = batch3.join(", ");
    db3.execute(&format!("INSERT INTO items VALUES {}", values3)).unwrap();

    // Verify all batches persist
    let count3 = db3.execute("SELECT COUNT(*) FROM items").unwrap();
    assert_eq!(count3.rows[0][0], vtdb::Value::Integer(2000));
}

#[test]
fn test_query_correctness_after_restart() {
    let (mut db, temp_dir) = create_test_database();
    let iceberg_path = temp_dir.path().join("iceberg");

    // Create table and insert data
    db.execute("CREATE TABLE products (id INTEGER, name VARCHAR, price INTEGER)").unwrap();
    db.execute("INSERT INTO products VALUES (1, 'Apple', 10), (2, 'Banana', 5), (3, 'Cherry', 15), (4, 'Date', 8)").unwrap();

    // Run various SELECT queries before restart
    let result1 = db.execute("SELECT * FROM products").unwrap();
    let result2 = db.execute("SELECT * FROM products WHERE price > 10").unwrap();
    let result3 = db.execute("SELECT * FROM products LIMIT 2").unwrap();
    let result4 = db.execute("SELECT name FROM products WHERE id = 2").unwrap();

    // Restart database
    let mut db2 = restart_test_database(&iceberg_path);

    // Run same queries again
    let result1_after = db2.execute("SELECT * FROM products").unwrap();
    let result2_after = db2.execute("SELECT * FROM products WHERE price > 10").unwrap();
    let result3_after = db2.execute("SELECT * FROM products LIMIT 2").unwrap();
    let result4_after = db2.execute("SELECT name FROM products WHERE id = 2").unwrap();

    // Verify results match exactly
    assert_eq!(result1.rows.len(), result1_after.rows.len());
    assert_eq!(result1.rows, result1_after.rows);
    
    assert_eq!(result2.rows.len(), result2_after.rows.len());
    assert_eq!(result2.rows, result2_after.rows);
    
    assert_eq!(result3.rows.len(), result3_after.rows.len());
    assert_eq!(result3.rows, result3_after.rows);
    
    assert_eq!(result4.rows.len(), result4_after.rows.len());
    assert_eq!(result4.rows, result4_after.rows);
}

#[test]
fn test_cache_behavior_with_iceberg_tables() {
    let (mut db, temp_dir) = create_test_database();
    let iceberg_path = temp_dir.path().join("iceberg");

    // Create table and insert data
    db.execute("CREATE TABLE cache_test (id INTEGER, value INTEGER)").unwrap();
    db.execute("INSERT INTO cache_test VALUES (1, 100), (2, 200)").unwrap();

    // Run SELECT query (should populate cache)
    let (result1, from_cache1) = db.execute_with_cache_info("SELECT * FROM cache_test ORDER BY id").unwrap();
    assert!(!from_cache1, "First query should not be from cache");
    assert_eq!(result1.rows.len(), 2);

    // Run same query again (should use cache)
    let (result2, from_cache2) = db.execute_with_cache_info("SELECT * FROM cache_test ORDER BY id").unwrap();
    assert!(from_cache2, "Second query should be from cache");
    assert_eq!(result2.rows.len(), 2);
    assert_eq!(result1.rows, result2.rows);

    // Insert new data
    db.execute("INSERT INTO cache_test VALUES (3, 300)").unwrap();

    // Run same SELECT query (should return fresh data, not cached)
    let (result3, from_cache3) = db.execute_with_cache_info("SELECT * FROM cache_test ORDER BY id").unwrap();
    assert!(!from_cache3, "Query after insert should not be from cache");
    assert_eq!(result3.rows.len(), 3, "Should return 3 rows after insert");
}

#[test]
fn test_large_dataset_persistence() {
    let (mut db, temp_dir) = create_test_database();
    let iceberg_path = temp_dir.path().join("iceberg");

    // Create table
    db.execute("CREATE TABLE large_table (id INTEGER, data VARCHAR)").unwrap();

    // Insert large dataset (10,000 rows)
    let mut values = Vec::new();
    for i in 1..=10000 {
        values.push(format!("({}, 'data_{}')", i, i));
    }
    
    // Insert in batches to avoid very long SQL strings
    for chunk in values.chunks(1000) {
        let sql = format!("INSERT INTO large_table VALUES {}", chunk.join(", "));
        db.execute(&sql).unwrap();
    }

    // Verify count before restart
    let count_before = db.execute("SELECT COUNT(*) FROM large_table").unwrap();
    assert_eq!(count_before.rows[0][0], vtdb::Value::Integer(10000));

    // Restart database
    let mut db2 = restart_test_database(&iceberg_path);

    // Verify all data is present
    let count_after = db2.execute("SELECT COUNT(*) FROM large_table").unwrap();
    assert_eq!(count_after.rows[0][0], vtdb::Value::Integer(10000));

    // Run SELECT with LIMIT and verify data integrity
    let result = db2.execute("SELECT * FROM large_table WHERE id = 5000").unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], vtdb::Value::Integer(5000));
    assert_eq!(result.rows[0][1], vtdb::Value::Varchar("data_5000".to_string()));

    // Verify row count method
    assert_eq!(db2.get_table_row_count("large_table").unwrap(), 10000);
}

#[test]
fn test_multiple_table_queries_after_restart() {
    let (mut db, temp_dir) = create_test_database();
    let iceberg_path = temp_dir.path().join("iceberg");

    // Create multiple tables with relationships
    db.execute("CREATE TABLE customers (id INTEGER, name VARCHAR)").unwrap();
    db.execute("CREATE TABLE orders (id INTEGER, customer_id INTEGER, amount INTEGER)").unwrap();

    // Insert data into all tables
    db.execute("INSERT INTO customers VALUES (1, 'Alice'), (2, 'Bob'), (3, 'Charlie')").unwrap();
    db.execute("INSERT INTO orders VALUES (101, 1, 100), (102, 1, 200), (103, 2, 150)").unwrap();

    // Restart database
    let mut db2 = restart_test_database(&iceberg_path);

    // Run JOIN queries across tables
    let join_result = db2.execute(
        "SELECT c.name, o.amount FROM customers c JOIN orders o ON c.id = o.customer_id ORDER BY o.amount"
    ).unwrap();
    
    assert_eq!(join_result.rows.len(), 3);
    assert_eq!(join_result.rows[0][0], vtdb::Value::Varchar("Alice".to_string()));
    assert_eq!(join_result.rows[0][1], vtdb::Value::Integer(100));
}

#[test]
fn test_row_count_accuracy() {
    let (mut db, temp_dir) = create_test_database();
    let iceberg_path = temp_dir.path().join("iceberg");

    // Create table
    db.execute("CREATE TABLE count_test (id INTEGER, value INTEGER)").unwrap();

    // Insert known number of rows
    db.execute("INSERT INTO count_test VALUES (1, 10), (2, 20), (3, 30), (4, 40), (5, 50)").unwrap();

    // Verify get_row_count returns correct count
    assert_eq!(db.get_table_row_count("count_test").unwrap(), 5);

    // Verify COUNT(*) query matches
    let count_query = db.execute("SELECT COUNT(*) FROM count_test").unwrap();
    assert_eq!(count_query.rows[0][0], vtdb::Value::Integer(5));

    // Restart database
    let mut db2 = restart_test_database(&iceberg_path);

    // Verify row count still correct
    assert_eq!(db2.get_table_row_count("count_test").unwrap(), 5);
    let count_query2 = db2.execute("SELECT COUNT(*) FROM count_test").unwrap();
    assert_eq!(count_query2.rows[0][0], vtdb::Value::Integer(5));

    // Insert more rows
    db2.execute("INSERT INTO count_test VALUES (6, 60), (7, 70)").unwrap();

    // Verify updated row count
    assert_eq!(db2.get_table_row_count("count_test").unwrap(), 7);
    let count_query3 = db2.execute("SELECT COUNT(*) FROM count_test").unwrap();
    assert_eq!(count_query3.rows[0][0], vtdb::Value::Integer(7));
}

