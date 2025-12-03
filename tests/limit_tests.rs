#[path = "test_utils.rs"]
mod test_utils;

use vtdb::{Database, Value};
use test_utils::*;

/// Test LIMIT functionality - same as Snowflake

#[test]
fn test_select_with_limit() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    // Insert more rows
    execute_query(&mut db, "INSERT INTO users VALUES (4, 'David')").unwrap();
    execute_query(&mut db, "INSERT INTO users VALUES (5, 'Eve')").unwrap();
    
    // Test LIMIT 2
    let result = execute_query(&mut db, "SELECT * FROM users LIMIT 2").unwrap();
    assert_row_count(&result, 2);
    assert_column_count(&result, 2);
}

#[test]
fn test_select_with_limit_and_where() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    // Insert more rows
    execute_query(&mut db, "INSERT INTO users VALUES (4, 'David')").unwrap();
    execute_query(&mut db, "INSERT INTO users VALUES (5, 'Eve')").unwrap();
    
    // Test LIMIT with WHERE clause
    let result = execute_query(&mut db, "SELECT * FROM users WHERE id > 1 LIMIT 2").unwrap();
    assert_row_count(&result, 2);
    
    // Verify we got rows with id > 1
    assert_cell_value(&result, 0, 0, &Value::Integer(2));
    assert_cell_value(&result, 1, 0, &Value::Integer(3));
}

#[test]
fn test_select_with_limit_exceeds_rows() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    // LIMIT larger than available rows should return all rows
    let result = execute_query(&mut db, "SELECT * FROM users LIMIT 100").unwrap();
    assert_row_count(&result, 3); // Only 3 rows exist
}

#[test]
fn test_select_with_limit_zero() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    // LIMIT 0 should return no rows
    let result = execute_query(&mut db, "SELECT * FROM users LIMIT 0").unwrap();
    assert_row_count(&result, 0);
}

#[test]
fn test_select_with_limit_one() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    // LIMIT 1 should return exactly 1 row
    let result = execute_query(&mut db, "SELECT * FROM users LIMIT 1").unwrap();
    assert_row_count(&result, 1);
    assert_cell_value(&result, 0, 0, &Value::Integer(1));
    assert_cell_value(&result, 0, 1, &Value::Varchar("Alice".to_string()));
}

#[test]
fn test_select_specific_columns_with_limit() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    // Test LIMIT with specific columns
    let result = execute_query(&mut db, "SELECT name FROM users LIMIT 2").unwrap();
    assert_row_count(&result, 2);
    assert_column_count(&result, 1);
    assert_eq!(result.columns, vec!["NAME"]);
}

#[test]
fn test_select_with_limit_equals_total_rows() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    // LIMIT equals total rows
    let result = execute_query(&mut db, "SELECT * FROM users LIMIT 3").unwrap();
    assert_row_count(&result, 3);
}

#[test]
fn test_select_with_limit_after_filtering() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    // Insert rows with different values
    execute_query(&mut db, "INSERT INTO users VALUES (4, 'Alice')").unwrap();
    execute_query(&mut db, "INSERT INTO users VALUES (5, 'Bob')").unwrap();
    
    // Filter and limit
    let result = execute_query(&mut db, "SELECT * FROM users WHERE name = 'Alice' LIMIT 1").unwrap();
    assert_row_count(&result, 1);
    assert_cell_value(&result, 0, 1, &Value::Varchar("Alice".to_string()));
}

