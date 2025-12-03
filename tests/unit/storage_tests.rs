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

#[test]
fn test_batch_insert_empty() {
    let mut storage = StorageEngine::new();
    storage.create_table("users".to_string(), 2).unwrap();
    
    // Insert empty batch should succeed
    storage.insert_rows("users", vec![]).unwrap();
    
    let rows = storage.scan_table("users").unwrap();
    assert_eq!(rows.len(), 0);
}

#[test]
fn test_batch_insert_large_batch() {
    let mut storage = StorageEngine::new();
    storage.create_table("users".to_string(), 3).unwrap();
    
    // Insert a large batch of rows
    let mut batch = Vec::new();
    for i in 0..1000 {
        batch.push(vec![
            Value::Integer(i),
            Value::Varchar(format!("User{}", i)),
            Value::Boolean(i % 2 == 0),
        ]);
    }
    
    storage.insert_rows("users", batch).unwrap();
    
    let rows = storage.scan_table("users").unwrap();
    assert_eq!(rows.len(), 1000);
    
    // Verify data integrity
    assert_eq!(rows[0][0], Value::Integer(0));
    assert_eq!(rows[999][0], Value::Integer(999));
    assert_eq!(rows[500][1], Value::Varchar("User500".to_string()));
}

#[test]
fn test_batch_insert_columnar_format() {
    let mut storage = StorageEngine::new();
    storage.create_table("test".to_string(), 2).unwrap();
    
    // Insert batch and verify columnar storage maintains row order
    storage.insert_rows("test", vec![
        vec![Value::Integer(1), Value::Varchar("A".to_string())],
        vec![Value::Integer(2), Value::Varchar("B".to_string())],
        vec![Value::Integer(3), Value::Varchar("C".to_string())],
    ]).unwrap();
    
    let rows = storage.scan_table("test").unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0], vec![Value::Integer(1), Value::Varchar("A".to_string())]);
    assert_eq!(rows[1], vec![Value::Integer(2), Value::Varchar("B".to_string())]);
    assert_eq!(rows[2], vec![Value::Integer(3), Value::Varchar("C".to_string())]);
}

#[test]
fn test_batch_insert_validation() {
    let mut storage = StorageEngine::new();
    storage.create_table("users".to_string(), 2).unwrap();
    
    // Try to insert batch with incorrect column count
    let result = storage.insert_rows("users", vec![
        vec![Value::Integer(1), Value::Varchar("Alice".to_string())],
        vec![Value::Integer(2)], // Wrong column count
    ]);
    
    assert!(result.is_err());
    
    // Verify no rows were inserted
    let rows = storage.scan_table("users").unwrap();
    assert_eq!(rows.len(), 0);
}

#[test]
fn test_batch_insert_all_data_types() {
    let mut storage = StorageEngine::new();
    storage.create_table("test".to_string(), 4).unwrap();
    
    // Test batch insert with all data types
    storage.insert_rows("test", vec![
        vec![
            Value::Integer(1),
            Value::Varchar("test".to_string()),
            Value::Boolean(true),
            Value::Null,
        ],
        vec![
            Value::Integer(-42),
            Value::Varchar("".to_string()), // Empty string
            Value::Boolean(false),
            Value::Integer(999), // Different type in same column
        ],
    ]).unwrap();
    
    let rows = storage.scan_table("test").unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][0], Value::Integer(1));
    assert_eq!(rows[0][1], Value::Varchar("test".to_string()));
    assert_eq!(rows[0][2], Value::Boolean(true));
    assert_eq!(rows[0][3], Value::Null);
}

#[test]
fn test_batch_insert_single_row() {
    let mut storage = StorageEngine::new();
    storage.create_table("users".to_string(), 2).unwrap();
    
    // Batch insert with single row should work
    storage.insert_rows("users", vec![
        vec![Value::Integer(1), Value::Varchar("Alice".to_string())],
    ]).unwrap();
    
    let rows = storage.scan_table("users").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0], Value::Integer(1));
}

#[test]
fn test_batch_insert_mixed_with_single_insert() {
    let mut storage = StorageEngine::new();
    storage.create_table("users".to_string(), 2).unwrap();
    
    // First, insert a single row using insert_row
    let table = storage.get_table("users").unwrap();
    table.insert_row(vec![Value::Integer(0), Value::Varchar("Zero".to_string())]).unwrap();
    
    // Then batch insert multiple rows
    storage.insert_rows("users", vec![
        vec![Value::Integer(1), Value::Varchar("One".to_string())],
        vec![Value::Integer(2), Value::Varchar("Two".to_string())],
        vec![Value::Integer(3), Value::Varchar("Three".to_string())],
    ]).unwrap();
    
    let rows = storage.scan_table("users").unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0][1], Value::Varchar("Zero".to_string()));
    assert_eq!(rows[1][1], Value::Varchar("One".to_string()));
    assert_eq!(rows[3][1], Value::Varchar("Three".to_string()));
}

#[test]
fn test_batch_insert_validation_first_row() {
    let mut storage = StorageEngine::new();
    storage.create_table("users".to_string(), 2).unwrap();
    
    // First row has wrong column count
    let result = storage.insert_rows("users", vec![
        vec![Value::Integer(1)], // Wrong column count
        vec![Value::Integer(2), Value::Varchar("Bob".to_string())],
    ]);
    
    assert!(result.is_err());
    let rows = storage.scan_table("users").unwrap();
    assert_eq!(rows.len(), 0);
}

#[test]
fn test_batch_insert_validation_middle_row() {
    let mut storage = StorageEngine::new();
    storage.create_table("users".to_string(), 2).unwrap();
    
    // Middle row has wrong column count
    let result = storage.insert_rows("users", vec![
        vec![Value::Integer(1), Value::Varchar("Alice".to_string())],
        vec![Value::Integer(2)], // Wrong column count
        vec![Value::Integer(3), Value::Varchar("Charlie".to_string())],
    ]);
    
    assert!(result.is_err());
    // Verify no rows were inserted (atomicity)
    let rows = storage.scan_table("users").unwrap();
    assert_eq!(rows.len(), 0);
}

#[test]
fn test_batch_insert_validation_last_row() {
    let mut storage = StorageEngine::new();
    storage.create_table("users".to_string(), 2).unwrap();
    
    // Last row has wrong column count
    let result = storage.insert_rows("users", vec![
        vec![Value::Integer(1), Value::Varchar("Alice".to_string())],
        vec![Value::Integer(2), Value::Varchar("Bob".to_string())],
        vec![Value::Integer(3)], // Wrong column count
    ]);
    
    assert!(result.is_err());
    let rows = storage.scan_table("users").unwrap();
    assert_eq!(rows.len(), 0);
}

#[test]
fn test_batch_insert_very_large_batch() {
    let mut storage = StorageEngine::new();
    storage.create_table("numbers".to_string(), 1).unwrap();
    
    // Insert a very large batch (10,000 rows)
    let mut batch = Vec::new();
    for i in 0..10000 {
        batch.push(vec![Value::Integer(i as i64)]);
    }
    
    storage.insert_rows("numbers", batch).unwrap();
    
    let rows = storage.scan_table("numbers").unwrap();
    assert_eq!(rows.len(), 10000);
    
    // Verify first and last rows
    assert_eq!(rows[0][0], Value::Integer(0));
    assert_eq!(rows[9999][0], Value::Integer(9999));
    assert_eq!(rows[5000][0], Value::Integer(5000));
}

#[test]
fn test_batch_insert_multiple_columns_large() {
    let mut storage = StorageEngine::new();
    storage.create_table("data".to_string(), 5).unwrap();
    
    // Insert large batch with many columns
    let mut batch = Vec::new();
    for i in 0..500 {
        batch.push(vec![
            Value::Integer(i),
            Value::Varchar(format!("Name{}", i)),
            Value::Boolean(i % 2 == 0),
            Value::Integer(i * 2),
            Value::Varchar(format!("Desc{}", i)),
        ]);
    }
    
    storage.insert_rows("data", batch).unwrap();
    
    let rows = storage.scan_table("data").unwrap();
    assert_eq!(rows.len(), 500);
    
    // Verify data integrity across columns
    assert_eq!(rows[0][0], Value::Integer(0));
    assert_eq!(rows[0][1], Value::Varchar("Name0".to_string()));
    assert_eq!(rows[499][0], Value::Integer(499));
    assert_eq!(rows[250][2], Value::Boolean(true));
}

