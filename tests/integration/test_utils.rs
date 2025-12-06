use vtdb::{Database, QueryResult, Value};
use anyhow::Result;
use tempfile::TempDir;
use std::sync::Mutex;

// Global test directory manager to ensure tests use isolated directories
lazy_static::lazy_static! {
    static ref TEST_DIRS: Mutex<Vec<TempDir>> = Mutex::new(Vec::new());
}

/// Create a new test database instance with isolated temporary directory
/// This ensures tests never touch production data in /data/ folder
pub fn create_test_db() -> Database {
    let temp_dir = TempDir::new().unwrap();
    let iceberg_path = temp_dir.path().join("iceberg");
    std::fs::create_dir_all(&iceberg_path).unwrap();
    
    // Keep temp_dir alive for the duration of tests
    TEST_DIRS.lock().unwrap().push(temp_dir);
    
    // Use test-only method to create database with custom path
    Database::with_iceberg_path(&iceberg_path).unwrap()
}

/// Execute a SQL query on a database and return the result
pub fn execute_query(db: &mut Database, sql: &str) -> Result<QueryResult> {
    db.execute(sql)
}

/// Assert that a query result has the expected number of rows
pub fn assert_row_count(result: &QueryResult, expected: usize) {
    assert_eq!(
        result.rows.len(),
        expected,
        "Expected {} rows, got {}",
        expected,
        result.rows.len()
    );
}

/// Assert that a query result has the expected number of columns
pub fn assert_column_count(result: &QueryResult, expected: usize) {
    assert_eq!(
        result.columns.len(),
        expected,
        "Expected {} columns, got {}",
        expected,
        result.columns.len()
    );
}

/// Assert that a specific cell value matches expected
pub fn assert_cell_value(result: &QueryResult, row: usize, col: usize, expected: &Value) {
    assert!(
        row < result.rows.len(),
        "Row index {} out of bounds ({} rows)",
        row,
        result.rows.len()
    );
    assert!(
        col < result.rows[row].len(),
        "Column index {} out of bounds ({} columns)",
        col,
        result.rows[row].len()
    );
    assert_eq!(
        &result.rows[row][col],
        expected,
        "Expected {:?}, got {:?} at row {}, col {}",
        expected,
        result.rows[row][col],
        row,
        col
    );
}

/// Assert that a query execution fails with an error
pub fn assert_query_error(db: &mut Database, sql: &str) {
    let result = db.execute(sql);
    assert!(result.is_err(), "Expected query to fail, but it succeeded: {}", sql);
}

/// Setup a test table with sample data
pub fn setup_test_table(db: &mut Database) -> Result<()> {
    // Create table
    db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)")?;
    
    // Insert test data
    db.execute("INSERT INTO users VALUES (1, 'Alice')")?;
    db.execute("INSERT INTO users VALUES (2, 'Bob')")?;
    db.execute("INSERT INTO users VALUES (3, 'Charlie')")?;
    
    Ok(())
}





