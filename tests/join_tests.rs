#[path = "test_utils.rs"]
mod test_utils;

use vtdb::{Database, Value};
use test_utils::*;

/// Test JOIN functionality - Snowflake compatible

#[test]
fn test_inner_join_basic() {
    let mut db = create_test_db();
    
    // Create two tables
    execute_query(&mut db, "CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    execute_query(&mut db, "CREATE TABLE orders (id INTEGER, user_id INTEGER, amount INTEGER)").unwrap();
    
    // Insert data
    execute_query(&mut db, "INSERT INTO users VALUES (1, 'Alice')").unwrap();
    execute_query(&mut db, "INSERT INTO users VALUES (2, 'Bob')").unwrap();
    execute_query(&mut db, "INSERT INTO orders VALUES (1, 1, 100)").unwrap();
    execute_query(&mut db, "INSERT INTO orders VALUES (2, 1, 200)").unwrap();
    execute_query(&mut db, "INSERT INTO orders VALUES (3, 2, 150)").unwrap();
    
    // Test INNER JOIN
    let result = execute_query(&mut db, "SELECT users.name, orders.amount FROM users INNER JOIN orders ON users.id = orders.user_id").unwrap();
    assert_row_count(&result, 3);
    assert_column_count(&result, 2);
}

#[test]
fn test_left_join() {
    let mut db = create_test_db();
    
    execute_query(&mut db, "CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    execute_query(&mut db, "CREATE TABLE orders (id INTEGER, user_id INTEGER, amount INTEGER)").unwrap();
    
    execute_query(&mut db, "INSERT INTO users VALUES (1, 'Alice')").unwrap();
    execute_query(&mut db, "INSERT INTO users VALUES (2, 'Bob')").unwrap();
    execute_query(&mut db, "INSERT INTO orders VALUES (1, 1, 100)").unwrap();
    
    // LEFT JOIN - should include Bob even though he has no orders
    let result = execute_query(&mut db, "SELECT users.name, orders.amount FROM users LEFT JOIN orders ON users.id = orders.user_id").unwrap();
    assert_row_count(&result, 2);
    // Verify we have both rows - one with amount 100 (Alice) and one with NULL (Bob)
    let has_alice = result.rows.iter().any(|row| {
        row.iter().any(|v| v == &Value::Varchar("Alice".to_string()))
    });
    let has_bob = result.rows.iter().any(|row| {
        row.iter().any(|v| v == &Value::Varchar("Bob".to_string()))
    });
    assert!(has_alice, "Alice's row should be present");
    assert!(has_bob, "Bob's row should be present");
    // At least one row should have NULL for amount (Bob's row)
    let has_null_amount = result.rows.iter().any(|row| {
        row.iter().any(|v| v == &Value::Null)
    });
    assert!(has_null_amount, "At least one row should have NULL amount");
}

#[test]
fn test_cross_join() {
    let mut db = create_test_db();
    
    execute_query(&mut db, "CREATE TABLE a (id INTEGER)").unwrap();
    execute_query(&mut db, "CREATE TABLE b (id INTEGER)").unwrap();
    
    execute_query(&mut db, "INSERT INTO a VALUES (1)").unwrap();
    execute_query(&mut db, "INSERT INTO a VALUES (2)").unwrap();
    execute_query(&mut db, "INSERT INTO b VALUES (10)").unwrap();
    execute_query(&mut db, "INSERT INTO b VALUES (20)").unwrap();
    
    // CROSS JOIN - should produce 2x2 = 4 rows
    let result = execute_query(&mut db, "SELECT * FROM a CROSS JOIN b").unwrap();
    assert_row_count(&result, 4);
}

#[test]
fn test_join_with_where() {
    let mut db = create_test_db();
    
    execute_query(&mut db, "CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    execute_query(&mut db, "CREATE TABLE orders (id INTEGER, user_id INTEGER, amount INTEGER)").unwrap();
    
    execute_query(&mut db, "INSERT INTO users VALUES (1, 'Alice')").unwrap();
    execute_query(&mut db, "INSERT INTO users VALUES (2, 'Bob')").unwrap();
    execute_query(&mut db, "INSERT INTO orders VALUES (1, 1, 100)").unwrap();
    execute_query(&mut db, "INSERT INTO orders VALUES (2, 1, 200)").unwrap();
    
    // JOIN with WHERE clause
    let result = execute_query(&mut db, "SELECT users.name FROM users INNER JOIN orders ON users.id = orders.user_id WHERE orders.amount > 150").unwrap();
    assert_row_count(&result, 1);
    assert_cell_value(&result, 0, 0, &Value::Varchar("Alice".to_string()));
}

#[test]
fn test_join_with_qualified_columns() {
    let mut db = create_test_db();
    
    execute_query(&mut db, "CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    execute_query(&mut db, "CREATE TABLE orders (id INTEGER, user_id INTEGER)").unwrap();
    
    execute_query(&mut db, "INSERT INTO users VALUES (1, 'Alice')").unwrap();
    execute_query(&mut db, "INSERT INTO orders VALUES (1, 1)").unwrap();
    
    // Test qualified column names in SELECT
    let result = execute_query(&mut db, "SELECT users.name, orders.id FROM users INNER JOIN orders ON users.id = orders.user_id").unwrap();
    assert_row_count(&result, 1);
    assert_column_count(&result, 2);
}

