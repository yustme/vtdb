use vtdb::catalog::{Catalog, types::DataType};

#[test]
fn test_create_table() {
    let mut catalog = Catalog::new();
    let result = catalog.create_table(
        "users".to_string(),
        vec![
            ("id".to_string(), DataType::Integer),
            ("name".to_string(), DataType::Varchar),
        ],
    );
    assert!(result.is_ok());
}

#[test]
fn test_get_table() {
    let mut catalog = Catalog::new();
    catalog.create_table(
        "users".to_string(),
        vec![("id".to_string(), DataType::Integer)],
    ).unwrap();
    
    let table = catalog.get_table("users");
    assert!(table.is_ok());
}

#[test]
fn test_table_exists() {
    let mut catalog = Catalog::new();
    catalog.create_table(
        "users".to_string(),
        vec![("id".to_string(), DataType::Integer)],
    ).unwrap();
    
    assert!(catalog.table_exists("users"));
    assert!(!catalog.table_exists("nonexistent"));
}

