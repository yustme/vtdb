use vtdb::{Database, Value};
use test_utils::*;

mod test_utils;

/// Test all example queries from the web interface
/// These queries should be executable directly in the Monaco editor

#[test]
fn test_example_create_table() {
    let mut db = create_test_db();
    
    // Example query: CREATE TABLE users (id INTEGER, name VARCHAR)
    let result = execute_query(&mut db, "CREATE TABLE users (id INTEGER, name VARCHAR)");
    assert!(result.is_ok(), "CREATE TABLE should succeed");
    
    // Verify table was created
    let result = execute_query(&mut db, "SELECT * FROM users").unwrap();
    assert_row_count(&result, 0);
    assert_column_count(&result, 2);
}

#[test]
fn test_example_insert() {
    let mut db = create_test_db();
    
    // Setup: Create table first
    execute_query(&mut db, "CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    
    // Example query: INSERT INTO users VALUES (1, 'Alice')
    let result = execute_query(&mut db, "INSERT INTO users VALUES (1, 'Alice')");
    assert!(result.is_ok(), "INSERT should succeed");
    
    // Verify data was inserted
    let result = execute_query(&mut db, "SELECT * FROM users").unwrap();
    assert_row_count(&result, 1);
    assert_cell_value(&result, 0, 0, &Value::Integer(1));
    assert_cell_value(&result, 0, 1, &Value::Varchar("Alice".to_string()));
}

#[test]
fn test_example_select_all() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    // Example query: SELECT * FROM users
    let result = execute_query(&mut db, "SELECT * FROM users").unwrap();
    
    assert_row_count(&result, 3);
    assert_column_count(&result, 2);
    assert_eq!(result.columns, vec!["ID", "NAME"]);
    
    // Verify data
    assert_cell_value(&result, 0, 0, &Value::Integer(1));
    assert_cell_value(&result, 0, 1, &Value::Varchar("Alice".to_string()));
    assert_cell_value(&result, 1, 0, &Value::Integer(2));
    assert_cell_value(&result, 1, 1, &Value::Varchar("Bob".to_string()));
}

#[test]
fn test_example_select_with_where() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    // Example query: SELECT name FROM users WHERE id = 1
    let result = execute_query(&mut db, "SELECT name FROM users WHERE id = 1").unwrap();
    
    assert_row_count(&result, 1);
    assert_column_count(&result, 1);
    assert_eq!(result.columns, vec!["NAME"]);
    assert_cell_value(&result, 0, 0, &Value::Varchar("Alice".to_string()));
}

#[test]
fn test_example_update() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    // Example query: UPDATE users SET name = 'Bob' WHERE id = 1
    let result = execute_query(&mut db, "UPDATE users SET name = 'Bob' WHERE id = 1");
    assert!(result.is_ok(), "UPDATE should succeed");
    
    // Verify update
    let result = execute_query(&mut db, "SELECT name FROM users WHERE id = 1").unwrap();
    assert_row_count(&result, 1);
    assert_cell_value(&result, 0, 0, &Value::Varchar("Bob".to_string()));
    
    // Verify other rows unchanged
    let result = execute_query(&mut db, "SELECT name FROM users WHERE id = 2").unwrap();
    assert_cell_value(&result, 0, 0, &Value::Varchar("Bob".to_string())); // Wait, this should still be "Bob" from setup
}

#[test]
fn test_example_delete() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    // Example query: DELETE FROM users WHERE id = 1
    let result = execute_query(&mut db, "DELETE FROM users WHERE id = 1");
    assert!(result.is_ok(), "DELETE should succeed");
    
    // Verify deletion
    let result = execute_query(&mut db, "SELECT * FROM users").unwrap();
    assert_row_count(&result, 2);
    
    // Verify the deleted row is gone
    let result = execute_query(&mut db, "SELECT * FROM users WHERE id = 1").unwrap();
    assert_row_count(&result, 0);
    
    // Verify remaining rows
    let result = execute_query(&mut db, "SELECT id FROM users").unwrap();
    assert_row_count(&result, 2);
    assert_cell_value(&result, 0, 0, &Value::Integer(2));
    assert_cell_value(&result, 1, 0, &Value::Integer(3));
}

#[test]
fn test_example_queries_workflow() {
    // Test that all example queries work together in sequence
    let mut db = create_test_db();
    
    // 1. CREATE TABLE
    execute_query(&mut db, "CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    
    // 2. INSERT
    execute_query(&mut db, "INSERT INTO users VALUES (1, 'Alice')").unwrap();
    
    // 3. SELECT ALL
    let result = execute_query(&mut db, "SELECT * FROM users").unwrap();
    assert_row_count(&result, 1);
    
    // 4. SELECT WITH WHERE
    let result = execute_query(&mut db, "SELECT name FROM users WHERE id = 1").unwrap();
    assert_row_count(&result, 1);
    assert_cell_value(&result, 0, 0, &Value::Varchar("Alice".to_string()));
    
    // 5. UPDATE
    execute_query(&mut db, "UPDATE users SET name = 'Bob' WHERE id = 1").unwrap();
    let result = execute_query(&mut db, "SELECT name FROM users WHERE id = 1").unwrap();
    assert_cell_value(&result, 0, 0, &Value::Varchar("Bob".to_string()));
    
    // 6. DELETE
    execute_query(&mut db, "DELETE FROM users WHERE id = 1").unwrap();
    let result = execute_query(&mut db, "SELECT * FROM users").unwrap();
    assert_row_count(&result, 0);
}

#[test]
fn test_example_queries_with_multiple_inserts() {
    // Test that INSERT example works multiple times
    let mut db = create_test_db();
    
    execute_query(&mut db, "CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    
    // Insert multiple times using the example query pattern
    execute_query(&mut db, "INSERT INTO users VALUES (1, 'Alice')").unwrap();
    execute_query(&mut db, "INSERT INTO users VALUES (2, 'Bob')").unwrap();
    execute_query(&mut db, "INSERT INTO users VALUES (3, 'Charlie')").unwrap();
    
    // Verify all inserts worked
    let result = execute_query(&mut db, "SELECT * FROM users").unwrap();
    assert_row_count(&result, 3);
}

