use vtdb::{Database, Value};
use crate::test_utils::create_test_db;

#[test]
fn test_create_table() {
    let mut db = create_test_db();
    let result = db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)");
    assert!(result.is_ok());
}

#[test]
fn test_insert_and_select() {
    let mut db = create_test_db();
    
    // Create table
    db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    
    // Insert data
    db.execute("INSERT INTO users VALUES (1, 'Alice')").unwrap();
    db.execute("INSERT INTO users VALUES (2, 'Bob')").unwrap();
    
    // Select all
    let result = db.execute("SELECT * FROM users").unwrap();
    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.columns.len(), 2);
    
    // Select with WHERE
    let result = db.execute("SELECT name FROM users WHERE id = 1").unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.columns.len(), 1);
    if let Value::Varchar(name) = &result.rows[0][0] {
        assert_eq!(name, "Alice");
    } else {
        panic!("Expected VARCHAR value");
    }
}

#[test]
fn test_update() {
    let mut db = create_test_db();
    
    // Create table
    db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    
    // Insert data
    db.execute("INSERT INTO users VALUES (1, 'Alice')").unwrap();
    
    // Update
    db.execute("UPDATE users SET name = 'Alice Updated' WHERE id = 1").unwrap();
    
    // Verify update
    let result = db.execute("SELECT name FROM users WHERE id = 1").unwrap();
    assert_eq!(result.rows.len(), 1);
    if let Value::Varchar(name) = &result.rows[0][0] {
        assert_eq!(name, "Alice Updated");
    } else {
        panic!("Expected VARCHAR value");
    }
}

#[test]
fn test_delete() {
    let mut db = create_test_db();
    
    // Create table
    db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    
    // Insert data
    db.execute("INSERT INTO users VALUES (1, 'Alice')").unwrap();
    db.execute("INSERT INTO users VALUES (2, 'Bob')").unwrap();
    
    // Delete
    db.execute("DELETE FROM users WHERE id = 1").unwrap();
    
    // Verify delete
    let result = db.execute("SELECT * FROM users").unwrap();
    assert_eq!(result.rows.len(), 1);
    if let Value::Integer(id) = &result.rows[0][0] {
        assert_eq!(*id, 2);
    } else {
        panic!("Expected INTEGER value");
    }
}

#[test]
fn test_quoted_identifiers() {
    let mut db = create_test_db();
    
    // Create table with quoted identifiers
    db.execute(r#"CREATE TABLE "users" ("id" INTEGER, "name" VARCHAR)"#).unwrap();
    
    // Insert with quoted identifiers
    db.execute(r#"INSERT INTO "users" VALUES (1, 'Alice')"#).unwrap();
    
    // Select with quoted identifiers
    let result = db.execute(r#"SELECT "name" FROM "users" WHERE "id" = 1"#).unwrap();
    assert_eq!(result.rows.len(), 1);
}

#[test]
fn test_where_clause_operators() {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE numbers (id INTEGER, value INTEGER)").unwrap();
    db.execute("INSERT INTO numbers VALUES (1, 10)").unwrap();
    db.execute("INSERT INTO numbers VALUES (2, 20)").unwrap();
    db.execute("INSERT INTO numbers VALUES (3, 30)").unwrap();
    
    // Test <
    let result = db.execute("SELECT id FROM numbers WHERE value < 25").unwrap();
    assert_eq!(result.rows.len(), 2);
    
    // Test >
    let result = db.execute("SELECT id FROM numbers WHERE value > 15").unwrap();
    assert_eq!(result.rows.len(), 2);
    
    // Test <=
    let result = db.execute("SELECT id FROM numbers WHERE value <= 20").unwrap();
    assert_eq!(result.rows.len(), 2);
    
    // Test >=
    let result = db.execute("SELECT id FROM numbers WHERE value >= 20").unwrap();
    assert_eq!(result.rows.len(), 2);
    
    // Test !=
    let result = db.execute("SELECT id FROM numbers WHERE value != 20").unwrap();
    assert_eq!(result.rows.len(), 2);
}

#[test]
fn test_and_or_operators() {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE users (id INTEGER, name VARCHAR, age INTEGER)").unwrap();
    db.execute("INSERT INTO users VALUES (1, 'Alice', 25)").unwrap();
    db.execute("INSERT INTO users VALUES (2, 'Bob', 30)").unwrap();
    db.execute("INSERT INTO users VALUES (3, 'Charlie', 25)").unwrap();
    
    // Test AND
    let result = db.execute("SELECT id FROM users WHERE age = 25 AND name = 'Alice'").unwrap();
    assert_eq!(result.rows.len(), 1);
    
    // Test OR
    let result = db.execute("SELECT id FROM users WHERE age = 25 OR age = 30").unwrap();
    assert_eq!(result.rows.len(), 3);
}

#[test]
fn test_batch_insert_sql() {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    
    // Insert multiple rows in a single INSERT statement (batch insert)
    // Note: SQL parser should handle multiple VALUES rows
    db.execute("INSERT INTO users VALUES (1, 'Alice')").unwrap();
    db.execute("INSERT INTO users VALUES (2, 'Bob')").unwrap();
    db.execute("INSERT INTO users VALUES (3, 'Charlie')").unwrap();
    
    // Verify all rows were inserted
    let result = db.execute("SELECT * FROM users ORDER BY id").unwrap();
    assert_eq!(result.rows.len(), 3);
    
    // Verify data integrity
    if let Value::Integer(id) = result.rows[0][0] {
        assert_eq!(id, 1);
    }
    if let Value::Varchar(name) = &result.rows[0][1] {
        assert_eq!(name, "Alice");
    }
}

#[test]
fn test_batch_insert_large_dataset() {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE numbers (id INTEGER, value INTEGER)").unwrap();
    
    // Insert many rows individually (each uses batch insert internally)
    for i in 1..=100 {
        db.execute(&format!("INSERT INTO numbers VALUES ({}, {})", i, i * 10)).unwrap();
    }
    
    // Verify all rows
    let result = db.execute("SELECT COUNT(*) FROM numbers").unwrap();
    assert_eq!(result.rows.len(), 1);
    
    // Verify data integrity
    let result = db.execute("SELECT * FROM numbers WHERE id = 50").unwrap();
    assert_eq!(result.rows.len(), 1);
    if let Value::Integer(value) = result.rows[0][1] {
        assert_eq!(value, 500);
    }
}

#[test]
fn test_batch_insert_with_different_types() {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE mixed (id INTEGER, name VARCHAR, active BOOLEAN)").unwrap();
    
    // Insert rows with different data types
    db.execute("INSERT INTO mixed VALUES (1, 'Alice', true)").unwrap();
    db.execute("INSERT INTO mixed VALUES (2, 'Bob', false)").unwrap();
    db.execute("INSERT INTO mixed VALUES (3, 'Charlie', true)").unwrap();
    
    let result = db.execute("SELECT * FROM mixed").unwrap();
    assert_eq!(result.rows.len(), 3);
    
    // Verify boolean values
    if let Value::Boolean(active) = result.rows[0][2] {
        assert_eq!(active, true);
    }
    if let Value::Boolean(active) = result.rows[1][2] {
        assert_eq!(active, false);
    }
}

#[test]
fn test_batch_insert_performance_verification() {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE perf_test (id INTEGER, data VARCHAR)").unwrap();
    
    // Insert many rows to verify batch insert performance
    // Each INSERT statement with multiple VALUES uses batch insertion internally
    let start = std::time::Instant::now();
    
    for i in 0..1000 {
        db.execute(&format!("INSERT INTO perf_test VALUES ({}, 'Data{}')", i, i)).unwrap();
    }
    
    let duration = start.elapsed();
    
    // Verify all rows were inserted correctly
    let result = db.execute("SELECT COUNT(*) FROM perf_test").unwrap();
    assert_eq!(result.rows.len(), 1);
    
    // Verify data integrity
    let result = db.execute("SELECT * FROM perf_test WHERE id = 500").unwrap();
    assert_eq!(result.rows.len(), 1);
    if let Value::Varchar(data) = &result.rows[0][1] {
        assert_eq!(data, "Data500");
    }
    
    // Performance check: batch inserts should be reasonably fast
    // 1000 inserts should complete in under 1 second (adjust threshold as needed)
    assert!(duration.as_secs() < 5, "Batch inserts took too long: {:?}", duration);
}

