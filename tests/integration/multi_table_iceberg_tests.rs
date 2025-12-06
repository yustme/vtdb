use anyhow::Result;
use vtdb::Database;
use vtdb::Value;
use crate::test_utils::create_test_db;

#[test]
fn test_multiple_tables_basic() -> Result<()> {
    let mut db = create_test_db();
    
    // Create multiple tables
    db.execute("CREATE TABLE table1 (id INTEGER, name VARCHAR)")?;
    db.execute("CREATE TABLE table2 (id INTEGER, value INTEGER)")?;
    db.execute("CREATE TABLE table3 (id INTEGER, data VARCHAR)")?;

    // Insert data into each table
    db.execute("INSERT INTO table1 VALUES (1, 'one'), (2, 'two'), (3, 'three')")?;
    db.execute("INSERT INTO table2 VALUES (10, 100), (20, 200), (30, 300)")?;
    db.execute("INSERT INTO table3 VALUES (100, 'data1'), (200, 'data2'), (300, 'data3')")?;

    // Force flush all tables
    db.storage.flush_table_iceberg_writes("table1")?;
    db.storage.flush_table_iceberg_writes("table2")?;
    db.storage.flush_table_iceberg_writes("table3")?;

    // Verify all tables have data
    let result1 = db.execute("SELECT COUNT(*) FROM table1")?;
    assert_eq!(result1.rows.len(), 1);
    if let Value::Integer(count) = result1.rows[0][0] {
        assert_eq!(count, 3, "table1 should have 3 rows");
    }

    let result2 = db.execute("SELECT COUNT(*) FROM table2")?;
    assert_eq!(result2.rows.len(), 1);
    if let Value::Integer(count) = result2.rows[0][0] {
        assert_eq!(count, 3, "table2 should have 3 rows");
    }

    let result3 = db.execute("SELECT COUNT(*) FROM table3")?;
    assert_eq!(result3.rows.len(), 1);
    if let Value::Integer(count) = result3.rows[0][0] {
        assert_eq!(count, 3, "table3 should have 3 rows");
    }

    // Verify specific data
    let result = db.execute("SELECT name FROM table1 WHERE id = 1")?;
    assert_eq!(result.rows.len(), 1);
    if let Value::Varchar(name) = &result.rows[0][0] {
        assert_eq!(name, "one");
    }

    let result = db.execute("SELECT value FROM table2 WHERE id = 20")?;
    assert_eq!(result.rows.len(), 1);
    if let Value::Integer(value) = result.rows[0][0] {
        assert_eq!(value, 200);
    }

    let result = db.execute("SELECT data FROM table3 WHERE id = 200")?;
    assert_eq!(result.rows.len(), 1);
    if let Value::Varchar(data) = &result.rows[0][0] {
        assert_eq!(data, "data2");
    }

    Ok(())
}

#[test]
fn test_multiple_tables_large_inserts() -> Result<()> {
    let mut db = create_test_db();
    
    // Create multiple tables
    db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)")?;
    db.execute("CREATE TABLE orders (id INTEGER, user_id INTEGER, amount INTEGER)")?;
    db.execute("CREATE TABLE products (id INTEGER, name VARCHAR, price INTEGER)")?;

    // Insert large batches into each table
    let mut user_values = Vec::new();
    for i in 1..=1000 {
        user_values.push(format!("({}, 'user_{}')", i, i));
    }
    db.execute(&format!("INSERT INTO users VALUES {}", user_values.join(", ")))?;

    let mut order_values = Vec::new();
    for i in 1..=500 {
        order_values.push(format!("({}, {}, {})", i, i % 1000 + 1, i * 10));
    }
    db.execute(&format!("INSERT INTO orders VALUES {}", order_values.join(", ")))?;

    let mut product_values = Vec::new();
    for i in 1..=200 {
        product_values.push(format!("({}, 'product_{}', {})", i, i, i * 100));
    }
    db.execute(&format!("INSERT INTO products VALUES {}", product_values.join(", ")))?;

    // Force flush all tables
    db.storage.flush_table_iceberg_writes("users")?;
    db.storage.flush_table_iceberg_writes("orders")?;
    db.storage.flush_table_iceberg_writes("products")?;

    // Verify all tables have correct row counts
    let result = db.execute("SELECT COUNT(*) FROM users")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 1000, "users table should have 1000 rows");
    }

    let result = db.execute("SELECT COUNT(*) FROM orders")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 500, "orders table should have 500 rows");
    }

    let result = db.execute("SELECT COUNT(*) FROM products")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 200, "products table should have 200 rows");
    }

    // Verify data integrity across tables
    let result = db.execute("SELECT name FROM users WHERE id = 500")?;
    assert_eq!(result.rows.len(), 1);
    if let Value::Varchar(name) = &result.rows[0][0] {
        assert_eq!(name, "user_500");
    }

    let result = db.execute("SELECT amount FROM orders WHERE id = 250")?;
    assert_eq!(result.rows.len(), 1);
    if let Value::Integer(amount) = result.rows[0][0] {
        assert_eq!(amount, 2500);
    }

    let result = db.execute("SELECT price FROM products WHERE id = 100")?;
    assert_eq!(result.rows.len(), 1);
    if let Value::Integer(price) = result.rows[0][0] {
        assert_eq!(price, 10000);
    }

    Ok(())
}

#[test]
fn test_multiple_tables_interleaved_inserts() -> Result<()> {
    let mut db = create_test_db();
    
    // Create multiple tables
    db.execute("CREATE TABLE A (id INTEGER)")?;
    db.execute("CREATE TABLE B (id INTEGER)")?;
    db.execute("CREATE TABLE C (id INTEGER)")?;

    // Interleave inserts across tables
    for i in 1..=30 {
        db.execute(&format!("INSERT INTO A VALUES ({})", i))?;
        db.execute(&format!("INSERT INTO B VALUES ({})", i * 2))?;
        db.execute(&format!("INSERT INTO C VALUES ({})", i * 3))?;
    }

    // Force flush all tables
    db.storage.flush_table_iceberg_writes("A")?;
    db.storage.flush_table_iceberg_writes("B")?;
    db.storage.flush_table_iceberg_writes("C")?;

    // Verify all tables have data
    let result = db.execute("SELECT COUNT(*) FROM A")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 30, "Table A should have 30 rows");
    }

    let result = db.execute("SELECT COUNT(*) FROM B")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 30, "Table B should have 30 rows");
    }

    let result = db.execute("SELECT COUNT(*) FROM C")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 30, "Table C should have 30 rows");
    }

    // Verify data values
    let result = db.execute("SELECT id FROM A WHERE id = 15")?;
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], Value::Integer(15));

    let result = db.execute("SELECT id FROM B WHERE id = 30")?;
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], Value::Integer(30));

    let result = db.execute("SELECT id FROM C WHERE id = 45")?;
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], Value::Integer(45));

    Ok(())
}

#[test]
fn test_table_isolation_after_flush() -> Result<()> {
    let mut db = create_test_db();
    
    // Create tables
    db.execute("CREATE TABLE X (id INTEGER, data VARCHAR)")?;
    db.execute("CREATE TABLE Y (id INTEGER, data VARCHAR)")?;

    // Insert into X and flush
    db.execute("INSERT INTO X VALUES (1, 'x1'), (2, 'x2')")?;
    db.storage.flush_table_iceberg_writes("X")?;

    // Insert into Y and flush
    db.execute("INSERT INTO Y VALUES (10, 'y1'), (20, 'y2')")?;
    db.storage.flush_table_iceberg_writes("Y")?;

    // Verify X still has its data
    let result = db.execute("SELECT COUNT(*) FROM X")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 2, "Table X should still have 2 rows after Y insert");
    }

    let result = db.execute("SELECT data FROM X WHERE id = 1")?;
    assert_eq!(result.rows.len(), 1);
    if let Value::Varchar(data) = &result.rows[0][0] {
        assert_eq!(data, "x1");
    }

    // Verify Y has its data
    let result = db.execute("SELECT COUNT(*) FROM Y")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 2, "Table Y should have 2 rows");
    }

    Ok(())
}

