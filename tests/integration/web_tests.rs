use vtdb::{Database, Value};
use test_utils::*;

mod test_utils;

#[test]
fn test_create_table() {
    let mut db = create_test_db();
    
    let result = execute_query(&mut db, "CREATE TABLE users (id INTEGER, name VARCHAR)");
    assert!(result.is_ok());
    
    // Verify table exists by querying it (should return empty result)
    let result = execute_query(&mut db, "SELECT * FROM users").unwrap();
    assert_row_count(&result, 0);
    assert_column_count(&result, 2);
    assert_eq!(result.columns, vec!["ID", "NAME"]);
}

#[test]
fn test_create_duplicate_table() {
    let mut db = create_test_db();
    
    execute_query(&mut db, "CREATE TABLE users (id INTEGER)").unwrap();
    
    // Try to create the same table again
    assert_query_error(&mut db, "CREATE TABLE users (id INTEGER)");
}

#[test]
fn test_insert_single_row() {
    let mut db = create_test_db();
    
    execute_query(&mut db, "CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    execute_query(&mut db, "INSERT INTO users VALUES (1, 'Alice')").unwrap();
    
    let result = execute_query(&mut db, "SELECT * FROM users").unwrap();
    assert_row_count(&result, 1);
    assert_cell_value(&result, 0, 0, &Value::Integer(1));
    assert_cell_value(&result, 0, 1, &Value::Varchar("Alice".to_string()));
}

#[test]
fn test_insert_multiple_rows() {
    let mut db = create_test_db();
    
    execute_query(&mut db, "CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    execute_query(&mut db, "INSERT INTO users VALUES (1, 'Alice')").unwrap();
    execute_query(&mut db, "INSERT INTO users VALUES (2, 'Bob')").unwrap();
    execute_query(&mut db, "INSERT INTO users VALUES (3, 'Charlie')").unwrap();
    
    let result = execute_query(&mut db, "SELECT * FROM users").unwrap();
    assert_row_count(&result, 3);
}

#[test]
fn test_select_all_columns() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    let result = execute_query(&mut db, "SELECT * FROM users").unwrap();
    assert_row_count(&result, 3);
    assert_column_count(&result, 2);
}

#[test]
fn test_select_specific_columns() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    let result = execute_query(&mut db, "SELECT name FROM users").unwrap();
    assert_row_count(&result, 3);
    assert_column_count(&result, 1);
    assert_eq!(result.columns, vec!["NAME"]);
}

#[test]
fn test_select_with_where_equals() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    let result = execute_query(&mut db, "SELECT name FROM users WHERE id = 1").unwrap();
    assert_row_count(&result, 1);
    assert_cell_value(&result, 0, 0, &Value::Varchar("Alice".to_string()));
}

#[test]
fn test_select_with_where_not_equals() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    let result = execute_query(&mut db, "SELECT name FROM users WHERE id != 1").unwrap();
    assert_row_count(&result, 2);
}

#[test]
fn test_select_with_where_less_than() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    let result = execute_query(&mut db, "SELECT id FROM users WHERE id < 3").unwrap();
    assert_row_count(&result, 2);
}

#[test]
fn test_select_with_where_greater_than() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    let result = execute_query(&mut db, "SELECT id FROM users WHERE id > 1").unwrap();
    assert_row_count(&result, 2);
}

#[test]
fn test_select_with_where_and() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    let result = execute_query(&mut db, "SELECT name FROM users WHERE id = 1 AND name = 'Alice'").unwrap();
    assert_row_count(&result, 1);
}

#[test]
fn test_select_with_where_or() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    let result = execute_query(&mut db, "SELECT id FROM users WHERE id = 1 OR id = 3").unwrap();
    assert_row_count(&result, 2);
}

#[test]
fn test_update_single_row() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    execute_query(&mut db, "UPDATE users SET name = 'Alice Updated' WHERE id = 1").unwrap();
    
    let result = execute_query(&mut db, "SELECT name FROM users WHERE id = 1").unwrap();
    assert_row_count(&result, 1);
    assert_cell_value(&result, 0, 0, &Value::Varchar("Alice Updated".to_string()));
}

#[test]
fn test_update_multiple_rows() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    execute_query(&mut db, "UPDATE users SET name = 'Updated' WHERE id < 3").unwrap();
    
    let result = execute_query(&mut db, "SELECT name FROM users WHERE id < 3").unwrap();
    assert_row_count(&result, 2);
    assert_cell_value(&result, 0, 0, &Value::Varchar("Updated".to_string()));
    assert_cell_value(&result, 1, 0, &Value::Varchar("Updated".to_string()));
}

#[test]
fn test_delete_single_row() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    execute_query(&mut db, "DELETE FROM users WHERE id = 1").unwrap();
    
    let result = execute_query(&mut db, "SELECT * FROM users").unwrap();
    assert_row_count(&result, 2);
}

#[test]
fn test_delete_multiple_rows() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    execute_query(&mut db, "DELETE FROM users WHERE id < 3").unwrap();
    
    let result = execute_query(&mut db, "SELECT * FROM users").unwrap();
    assert_row_count(&result, 1);
}

#[test]
fn test_delete_all_rows() {
    let mut db = create_test_db();
    setup_test_table(&mut db).unwrap();
    
    execute_query(&mut db, "DELETE FROM users").unwrap();
    
    let result = execute_query(&mut db, "SELECT * FROM users").unwrap();
    assert_row_count(&result, 0);
}

#[test]
fn test_error_invalid_sql() {
    let mut db = create_test_db();
    assert_query_error(&mut db, "INVALID SQL STATEMENT");
}

#[test]
fn test_error_table_not_found() {
    let mut db = create_test_db();
    assert_query_error(&mut db, "SELECT * FROM nonexistent");
}

#[test]
fn test_error_column_not_found() {
    let mut db = create_test_db();
    execute_query(&mut db, "CREATE TABLE users (id INTEGER)").unwrap();
    assert_query_error(&mut db, "SELECT invalid_column FROM users");
}

#[test]
fn test_workflow_create_insert_select() {
    let mut db = create_test_db();
    
    // Create table
    execute_query(&mut db, "CREATE TABLE products (id INTEGER, name VARCHAR, price INTEGER)").unwrap();
    
    // Insert data
    execute_query(&mut db, "INSERT INTO products VALUES (1, 'Laptop', 999)").unwrap();
    execute_query(&mut db, "INSERT INTO products VALUES (2, 'Mouse', 25)").unwrap();
    
    // Query data
    let result = execute_query(&mut db, "SELECT name, price FROM products WHERE price > 50").unwrap();
    assert_row_count(&result, 1);
    assert_cell_value(&result, 0, 0, &Value::Varchar("Laptop".to_string()));
    assert_cell_value(&result, 0, 1, &Value::Integer(999));
}

#[test]
fn test_workflow_create_insert_update_select() {
    let mut db = create_test_db();
    
    execute_query(&mut db, "CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    execute_query(&mut db, "INSERT INTO users VALUES (1, 'Alice')").unwrap();
    execute_query(&mut db, "UPDATE users SET name = 'Bob' WHERE id = 1").unwrap();
    
    let result = execute_query(&mut db, "SELECT name FROM users WHERE id = 1").unwrap();
    assert_row_count(&result, 1);
    assert_cell_value(&result, 0, 0, &Value::Varchar("Bob".to_string()));
}

#[test]
fn test_workflow_create_insert_delete_select() {
    let mut db = create_test_db();
    
    execute_query(&mut db, "CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    execute_query(&mut db, "INSERT INTO users VALUES (1, 'Alice')").unwrap();
    execute_query(&mut db, "INSERT INTO users VALUES (2, 'Bob')").unwrap();
    execute_query(&mut db, "DELETE FROM users WHERE id = 1").unwrap();
    
    let result = execute_query(&mut db, "SELECT * FROM users").unwrap();
    assert_row_count(&result, 1);
    assert_cell_value(&result, 0, 0, &Value::Integer(2));
}

#[test]
fn test_multiple_tables() {
    let mut db = create_test_db();
    
    execute_query(&mut db, "CREATE TABLE users (id INTEGER, name VARCHAR)").unwrap();
    execute_query(&mut db, "CREATE TABLE orders (id INTEGER, user_id INTEGER)").unwrap();
    
    execute_query(&mut db, "INSERT INTO users VALUES (1, 'Alice')").unwrap();
    execute_query(&mut db, "INSERT INTO orders VALUES (1, 1)").unwrap();
    
    let users_result = execute_query(&mut db, "SELECT * FROM users").unwrap();
    assert_row_count(&users_result, 1);
    
    let orders_result = execute_query(&mut db, "SELECT * FROM orders").unwrap();
    assert_row_count(&orders_result, 1);
}

#[test]
fn test_quoted_identifiers() {
    let mut db = create_test_db();
    
    execute_query(&mut db, r#"CREATE TABLE "users" ("id" INTEGER, "name" VARCHAR)"#).unwrap();
    execute_query(&mut db, r#"INSERT INTO "users" VALUES (1, 'Alice')"#).unwrap();
    
    let result = execute_query(&mut db, r#"SELECT "name" FROM "users" WHERE "id" = 1"#).unwrap();
    assert_row_count(&result, 1);
    assert_cell_value(&result, 0, 0, &Value::Varchar("Alice".to_string()));
}

