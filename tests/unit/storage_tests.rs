use vtdb::storage::StorageEngine;
use vtdb::Value;

#[test]
fn test_create_table() {
    let mut storage = StorageEngine::new();
    let result = storage.create_table("users".to_string(), 2);
    assert!(result.is_ok());
}

#[test]
fn test_insert_and_scan() {
    let mut storage = StorageEngine::new();
    storage.create_table("users".to_string(), 2).unwrap();
    
    storage.insert_rows("users", vec![
        vec![Value::Integer(1), Value::Varchar("Alice".to_string())],
        vec![Value::Integer(2), Value::Varchar("Bob".to_string())],
    ]).unwrap();
    
    let rows = storage.scan_table("users").unwrap();
    assert_eq!(rows.len(), 2);
}

#[test]
fn test_update_rows() {
    let mut storage = StorageEngine::new();
    storage.create_table("users".to_string(), 2).unwrap();
    
    storage.insert_rows("users", vec![
        vec![Value::Integer(1), Value::Varchar("Alice".to_string())],
    ]).unwrap();
    
    let updated = storage.update_rows(
        "users",
        1,
        Value::Varchar("Alice Updated".to_string()),
        |row| row[0] == Value::Integer(1),
    ).unwrap();
    
    assert_eq!(updated, 1);
}

#[test]
fn test_delete_rows() {
    let mut storage = StorageEngine::new();
    storage.create_table("users".to_string(), 2).unwrap();
    
    storage.insert_rows("users", vec![
        vec![Value::Integer(1), Value::Varchar("Alice".to_string())],
        vec![Value::Integer(2), Value::Varchar("Bob".to_string())],
    ]).unwrap();
    
    let deleted = storage.delete_rows(
        "users",
        |row| row[0] == Value::Integer(1),
    ).unwrap();
    
    assert_eq!(deleted, 1);
    
    let rows = storage.scan_table("users").unwrap();
    assert_eq!(rows.len(), 1);
}

