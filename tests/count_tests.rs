#[path = "test_utils.rs"]
mod test_utils;

use vtdb::{Database, Value};
use test_utils::*;

/// Test COUNT() function - Snowflake compatible

#[test]
fn test_count_star() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    // COUNT(*) returns total row count
    let result = execute_query(&mut db, "SELECT COUNT(*) FROM users").unwrap();
    assert_row_count(&result, 1);
    assert_column_count(&result, 1);
    assert_cell_value(&result, 0, 0, &Value::Integer(3));
}

#[test]
fn test_count_star_empty() {
    let mut db = create_test_db();
    execute_query(&mut db, "CREATE TABLE empty_table (id INTEGER)").unwrap();
    
    // COUNT(*) on empty table returns 0
    let result = execute_query(&mut db, "SELECT COUNT(*) FROM empty_table").unwrap();
    assert_row_count(&result, 1);
    assert_cell_value(&result, 0, 0, &Value::Integer(0));
}

#[test]
fn test_count_column() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    // COUNT(column) counts non-NULL values
    let result = execute_query(&mut db, "SELECT COUNT(name) FROM users").unwrap();
    assert_row_count(&result, 1);
    assert_cell_value(&result, 0, 0, &Value::Integer(3));
}

#[test]
fn test_count_column_with_nulls() {
    let mut db = create_test_db();
    execute_query(&mut db, "CREATE TABLE test (id INTEGER, name VARCHAR)").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (1, 'Alice')").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (2, NULL)").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (3, 'Bob')").unwrap();
    
    // COUNT(name) ignores NULL values
    let result = execute_query(&mut db, "SELECT COUNT(name) FROM test").unwrap();
    assert_cell_value(&result, 0, 0, &Value::Integer(2));
    
    // COUNT(*) counts all rows including NULLs
    let result = execute_query(&mut db, "SELECT COUNT(*) FROM test").unwrap();
    assert_cell_value(&result, 0, 0, &Value::Integer(3));
}

#[test]
fn test_count_distinct() {
    let mut db = create_test_db();
    execute_query(&mut db, "CREATE TABLE test (id INTEGER, category VARCHAR)").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (1, 'A')").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (2, 'A')").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (3, 'B')").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (4, 'B')").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (5, 'C')").unwrap();
    
    // COUNT(DISTINCT category) counts unique values
    let result = execute_query(&mut db, "SELECT COUNT(DISTINCT category) FROM test").unwrap();
    assert_cell_value(&result, 0, 0, &Value::Integer(3));
}

#[test]
fn test_count_with_where() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    // COUNT(*) with WHERE clause
    let result = execute_query(&mut db, "SELECT COUNT(*) FROM users WHERE id > 1").unwrap();
    assert_cell_value(&result, 0, 0, &Value::Integer(2));
}

#[test]
fn test_count_column_with_where() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    // COUNT(column) with WHERE clause
    let result = execute_query(&mut db, "SELECT COUNT(name) FROM users WHERE id < 3").unwrap();
    assert_cell_value(&result, 0, 0, &Value::Integer(2));
}

#[test]
fn test_count_with_group_by() {
    let mut db = create_test_db();
    execute_query(&mut db, "CREATE TABLE sales (id INTEGER, category VARCHAR, amount INTEGER)").unwrap();
    execute_query(&mut db, "INSERT INTO sales VALUES (1, 'A', 100)").unwrap();
    execute_query(&mut db, "INSERT INTO sales VALUES (2, 'A', 200)").unwrap();
    execute_query(&mut db, "INSERT INTO sales VALUES (3, 'B', 150)").unwrap();
    execute_query(&mut db, "INSERT INTO sales VALUES (4, 'B', 250)").unwrap();
    
    // COUNT(*) grouped by category
    let result = execute_query(&mut db, "SELECT category, COUNT(*) FROM sales GROUP BY category").unwrap();
    assert_row_count(&result, 2);
    assert_column_count(&result, 2);
    
    // Verify results (order may vary)
    let mut found_a = false;
    let mut found_b = false;
    for row in &result.rows {
        if row[1] == Value::Integer(2) {
            if row[0] == Value::Varchar("A".to_string()) {
                found_a = true;
            } else if row[0] == Value::Varchar("B".to_string()) {
                found_b = true;
            }
        }
    }
    assert!(found_a && found_b, "Expected both categories with count 2");
}

#[test]
fn test_count_column_with_group_by() {
    let mut db = create_test_db();
    execute_query(&mut db, "CREATE TABLE test (id INTEGER, category VARCHAR, value INTEGER)").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (1, 'A', 10)").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (2, 'A', NULL)").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (3, 'A', 20)").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (4, 'B', 30)").unwrap();
    
    // COUNT(value) grouped by category
    let result = execute_query(&mut db, "SELECT category, COUNT(value) FROM test GROUP BY category").unwrap();
    assert_row_count(&result, 2);
    
    // Category A should have count 2 (one NULL ignored)
    // Category B should have count 1
    let mut found_a = false;
    let mut found_b = false;
    for row in &result.rows {
        if row[0] == Value::Varchar("A".to_string()) {
            assert_eq!(row[1], Value::Integer(2));
            found_a = true;
        } else if row[0] == Value::Varchar("B".to_string()) {
            assert_eq!(row[1], Value::Integer(1));
            found_b = true;
        }
    }
    assert!(found_a && found_b);
}

#[test]
fn test_count_distinct_with_group_by() {
    let mut db = create_test_db();
    execute_query(&mut db, "CREATE TABLE test (id INTEGER, category VARCHAR, value VARCHAR)").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (1, 'A', 'X')").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (2, 'A', 'X')").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (3, 'A', 'Y')").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (4, 'B', 'Z')").unwrap();
    
    // COUNT(DISTINCT value) grouped by category
    let result = execute_query(&mut db, "SELECT category, COUNT(DISTINCT value) FROM test GROUP BY category").unwrap();
    assert_row_count(&result, 2);
    
    // Category A should have 2 distinct values (X, Y)
    // Category B should have 1 distinct value (Z)
    let mut found_a = false;
    let mut found_b = false;
    for row in &result.rows {
        if row[0] == Value::Varchar("A".to_string()) {
            assert_eq!(row[1], Value::Integer(2));
            found_a = true;
        } else if row[0] == Value::Varchar("B".to_string()) {
            assert_eq!(row[1], Value::Integer(1));
            found_b = true;
        }
    }
    assert!(found_a && found_b);
}

#[test]
fn test_group_by_multiple_columns() {
    let mut db = create_test_db();
    execute_query(&mut db, "CREATE TABLE test (id INTEGER, cat1 VARCHAR, cat2 VARCHAR)").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (1, 'A', 'X')").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (2, 'A', 'X')").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (3, 'A', 'Y')").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (4, 'B', 'X')").unwrap();
    
    // GROUP BY multiple columns
    let result = execute_query(&mut db, "SELECT cat1, cat2, COUNT(*) FROM test GROUP BY cat1, cat2").unwrap();
    assert_row_count(&result, 3); // (A,X), (A,Y), (B,X)
}

#[test]
fn test_count_all_nulls() {
    let mut db = create_test_db();
    execute_query(&mut db, "CREATE TABLE test (id INTEGER, name VARCHAR)").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (1, NULL)").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (2, NULL)").unwrap();
    execute_query(&mut db, "INSERT INTO test VALUES (3, NULL)").unwrap();
    
    // COUNT(name) when all values are NULL should return 0
    let result = execute_query(&mut db, "SELECT COUNT(name) FROM test").unwrap();
    assert_cell_value(&result, 0, 0, &Value::Integer(0));
    
    // COUNT(*) should still count all rows
    let result = execute_query(&mut db, "SELECT COUNT(*) FROM test").unwrap();
    assert_cell_value(&result, 0, 0, &Value::Integer(3));
}

#[test]
fn test_count_with_limit() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    // COUNT() should ignore LIMIT (aggregation happens first)
    let result = execute_query(&mut db, "SELECT COUNT(*) FROM users LIMIT 1").unwrap();
    assert_cell_value(&result, 0, 0, &Value::Integer(3));
    assert_row_count(&result, 1);
}

