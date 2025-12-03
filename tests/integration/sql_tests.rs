use vtdb::{Database, Value};

#[test]
fn test_create_table() {
    let mut db = Database::new();
    let result = db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)");
    assert!(result.is_ok());
}

#[test]
fn test_insert_and_select() {
    let mut db = Database::new();
    
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
    let mut db = Database::new();
    
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
    let mut db = Database::new();
    
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
    let mut db = Database::new();
    
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
    let mut db = Database::new();
    
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
    let mut db = Database::new();
    
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

