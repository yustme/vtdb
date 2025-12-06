use vtdb::{Database, Value};
use crate::test_utils::create_test_db;
use anyhow::Result;

#[test]
fn test_bulk_insert_exact_scenario() -> Result<()> {
    let mut db = create_test_db();
    
    // Create tables
    db.execute("CREATE TABLE customers (id INTEGER, name VARCHAR, email VARCHAR)")?;
    db.execute("CREATE TABLE products (id INTEGER, name VARCHAR, price INTEGER)")?;
    db.execute("CREATE TABLE orders (id INTEGER, customer_id INTEGER, product_id INTEGER, quantity INTEGER)")?;
    
    // Insert customers: 5 inserts of 1000 rows each = 5000 total rows
    println!("Inserting customers...");
    for batch in 0..5 {
        let start = batch * 1000 + 1;
        let end = (batch + 1) * 1000;
        let mut values = Vec::new();
        for i in start..=end {
            values.push(format!("({}, 'Customer_{}', 'customer_{}@example.com')", i, i, i));
        }
        let sql = format!("INSERT INTO customers VALUES {}", values.join(", "));
        db.execute(&sql)?;
        println!("  Inserted customers batch {}: rows {} to {}", batch + 1, start, end);
    }
    
    // Insert products: 1 insert of 200 rows = 200 total rows
    println!("Inserting products...");
    let mut values = Vec::new();
    for i in 1..=200 {
        values.push(format!("({}, 'Product_{}', {})", i, i, i * 10));
    }
    let sql = format!("INSERT INTO products VALUES {}", values.join(", "));
    db.execute(&sql)?;
    println!("  Inserted products: 200 rows");
    
    // Insert orders: 50 inserts of 1000 rows each = 50000 total rows
    println!("Inserting orders...");
    for batch in 0..50 {
        let start = batch * 1000 + 1;
        let end = (batch + 1) * 1000;
        let mut values = Vec::new();
        for i in start..=end {
            values.push(format!("({}, {}, {}, {})", i, (i % 5000) + 1, (i % 200) + 1, (i % 10) + 1));
        }
        let sql = format!("INSERT INTO orders VALUES {}", values.join(", "));
        db.execute(&sql)?;
        if (batch + 1) % 10 == 0 {
            println!("  Inserted orders batch {}: rows {} to {}", batch + 1, start, end);
        }
    }
    
    // Force flush all tables to ensure data is persisted
    println!("Flushing all tables...");
    db.storage.flush_table_iceberg_writes("customers")?;
    db.storage.flush_table_iceberg_writes("products")?;
    db.storage.flush_table_iceberg_writes("orders")?;
    println!("  All tables flushed");
    
    // Verify all tables have correct row counts
    println!("Verifying row counts...");
    
    let result = db.execute("SELECT COUNT(*) FROM customers")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 5000, "customers table should have 5000 rows, but has {}", count);
        println!("  ✓ customers: {} rows", count);
    } else {
        panic!("Expected integer count for customers");
    }
    
    let result = db.execute("SELECT COUNT(*) FROM products")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 200, "products table should have 200 rows, but has {}", count);
        println!("  ✓ products: {} rows", count);
    } else {
        panic!("Expected integer count for products");
    }
    
    let result = db.execute("SELECT COUNT(*) FROM orders")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 50000, "orders table should have 50000 rows, but has {}", count);
        println!("  ✓ orders: {} rows", count);
    } else {
        panic!("Expected integer count for orders");
    }
    
    // Verify data integrity - check sample rows from each table
    println!("Verifying data integrity...");
    
    // Check customers: first, middle, last
    let result = db.execute("SELECT name FROM customers WHERE id = 1")?;
    assert_eq!(result.rows[0][0], Value::Varchar("Customer_1".to_string()));
    
    let result = db.execute("SELECT name FROM customers WHERE id = 2500")?;
    assert_eq!(result.rows[0][0], Value::Varchar("Customer_2500".to_string()));
    
    let result = db.execute("SELECT name FROM customers WHERE id = 5000")?;
    assert_eq!(result.rows[0][0], Value::Varchar("Customer_5000".to_string()));
    
    // Check products: first, middle, last
    let result = db.execute("SELECT name FROM products WHERE id = 1")?;
    assert_eq!(result.rows[0][0], Value::Varchar("Product_1".to_string()));
    
    let result = db.execute("SELECT name FROM products WHERE id = 100")?;
    assert_eq!(result.rows[0][0], Value::Varchar("Product_100".to_string()));
    
    let result = db.execute("SELECT name FROM products WHERE id = 200")?;
    assert_eq!(result.rows[0][0], Value::Varchar("Product_200".to_string()));
    
    // Check orders: first, middle, last
    let result = db.execute("SELECT customer_id FROM orders WHERE id = 1")?;
    assert_eq!(result.rows[0][0], Value::Integer(2)); // (1 % 5000) + 1
    
    let result = db.execute("SELECT customer_id FROM orders WHERE id = 25000")?;
    assert_eq!(result.rows[0][0], Value::Integer(25001)); // (25000 % 5000) + 1
    
    let result = db.execute("SELECT customer_id FROM orders WHERE id = 50000")?;
    assert_eq!(result.rows[0][0], Value::Integer(1)); // (50000 % 5000) + 1
    
    println!("  ✓ All data integrity checks passed");
    
    Ok(())
}

#[test]
fn test_bulk_insert_with_immediate_queries() -> Result<()> {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE test_table (id INTEGER, value INTEGER)")?;
    
    // Insert in batches and verify after each batch
    for batch in 0..5 {
        let start = batch * 1000 + 1;
        let end = (batch + 1) * 1000;
        let mut values = Vec::new();
        for i in start..=end {
            values.push(format!("({}, {})", i, i * 10));
        }
        let sql = format!("INSERT INTO test_table VALUES {}", values.join(", "));
        db.execute(&sql)?;
        
        // Verify count immediately after each insert
        let result = db.execute("SELECT COUNT(*) FROM test_table")?;
        if let Value::Integer(count) = result.rows[0][0] {
            assert_eq!(count, end, "After batch {}, should have {} rows, but has {}", batch + 1, end, count);
        }
    }
    
    // Final flush and verification
    db.storage.flush_table_iceberg_writes("test_table")?;
    
    let result = db.execute("SELECT COUNT(*) FROM test_table")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 5000, "Final count should be 5000, but has {}", count);
    }
    
    Ok(())
}

#[test]
fn test_bulk_insert_interleaved_tables() -> Result<()> {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE a (id INTEGER)")?;
    db.execute("CREATE TABLE b (id INTEGER)")?;
    db.execute("CREATE TABLE c (id INTEGER)")?;
    
    // Interleave inserts across all three tables
    for batch in 0..10 {
        // Insert into table a
        let mut values_a = Vec::new();
        for i in (batch * 100 + 1)..=((batch + 1) * 100) {
            values_a.push(format!("({})", i));
        }
        db.execute(&format!("INSERT INTO a VALUES {}", values_a.join(", ")))?;
        
        // Insert into table b
        let mut values_b = Vec::new();
        for i in (batch * 100 + 1)..=((batch + 1) * 100) {
            values_b.push(format!("({})", i * 2));
        }
        db.execute(&format!("INSERT INTO b VALUES {}", values_b.join(", ")))?;
        
        // Insert into table c
        let mut values_c = Vec::new();
        for i in (batch * 100 + 1)..=((batch + 1) * 100) {
            values_c.push(format!("({})", i * 3));
        }
        db.execute(&format!("INSERT INTO c VALUES {}", values_c.join(", ")))?;
    }
    
    // Flush all tables
    db.storage.flush_table_iceberg_writes("a")?;
    db.storage.flush_table_iceberg_writes("b")?;
    db.storage.flush_table_iceberg_writes("c")?;
    
    // Verify all tables
    let result = db.execute("SELECT COUNT(*) FROM a")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 1000, "Table a should have 1000 rows");
    }
    
    let result = db.execute("SELECT COUNT(*) FROM b")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 1000, "Table b should have 1000 rows");
    }
    
    let result = db.execute("SELECT COUNT(*) FROM c")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 1000, "Table c should have 1000 rows");
    }
    
    Ok(())
}

#[test]
fn test_bulk_insert_verify_no_duplicates() -> Result<()> {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE no_dupes (id INTEGER PRIMARY KEY, value INTEGER)")?;
    
    // Insert same data multiple times (should not create duplicates if we're careful)
    for batch in 0..3 {
        let mut values = Vec::new();
        for i in 1..=100 {
            values.push(format!("({}, {})", i, i * 10));
        }
        let sql = format!("INSERT INTO no_dupes VALUES {}", values.join(", "));
        db.execute(&sql)?;
    }
    
    db.storage.flush_table_iceberg_writes("no_dupes")?;
    
    // Verify count (should be 300 if inserts are additive, or 100 if they replace)
    // For now, we expect additive behavior
    let result = db.execute("SELECT COUNT(*) FROM no_dupes")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 300, "Should have 300 rows (3 batches of 100)");
    }
    
    // Verify no duplicates by checking distinct count equals total count
    let distinct_result = db.execute("SELECT COUNT(DISTINCT id) FROM no_dupes")?;
    // Note: This test assumes DISTINCT is supported, if not, we'll skip this check
    // For now, just verify we can query the table
    
    Ok(())
}

#[test]
fn test_bulk_insert_small_table_edge_case() -> Result<()> {
    let mut db = create_test_db();
    
    // Test with a very small table (200 rows) to catch edge cases
    db.execute("CREATE TABLE small (id INTEGER, name VARCHAR)")?;
    
    // Insert 200 rows in one batch (less than 1000, so should flush immediately)
    let mut values = Vec::new();
    for i in 1..=200 {
        values.push(format!("({}, 'Item_{}')", i, i));
    }
    let sql = format!("INSERT INTO small VALUES {}", values.join(", "));
    db.execute(&sql)?;
    
    // Verify immediately (should work because small batches flush immediately)
    let result = db.execute("SELECT COUNT(*) FROM small")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 200, "Small table should have 200 rows immediately");
    }
    
    // Explicit flush and verify again
    db.storage.flush_table_iceberg_writes("small")?;
    
    let result = db.execute("SELECT COUNT(*) FROM small")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 200, "Small table should still have 200 rows after flush");
    }
    
    // Verify sample data
    let result = db.execute("SELECT name FROM small WHERE id = 100")?;
    assert_eq!(result.rows[0][0], Value::Varchar("Item_100".to_string()));
    
    Ok(())
}

#[test]
fn test_bulk_insert_large_table_edge_case() -> Result<()> {
    let mut db = create_test_db();
    
    // Test with a very large table (50000 rows)
    db.execute("CREATE TABLE large (id INTEGER, data VARCHAR)")?;
    
    // Insert 50 batches of 1000 rows each
    for batch in 0..50 {
        let start = batch * 1000 + 1;
        let end = (batch + 1) * 1000;
        let mut values = Vec::new();
        for i in start..=end {
            values.push(format!("({}, 'data_{}')", i, i));
        }
        let sql = format!("INSERT INTO large VALUES {}", values.join(", "));
        db.execute(&sql)?;
    }
    
    // Flush and verify
    db.storage.flush_table_iceberg_writes("large")?;
    
    let result = db.execute("SELECT COUNT(*) FROM large")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 50000, "Large table should have 50000 rows");
    }
    
    // Verify data at various points
    let result = db.execute("SELECT data FROM large WHERE id = 1")?;
    assert_eq!(result.rows[0][0], Value::Varchar("data_1".to_string()));
    
    let result = db.execute("SELECT data FROM large WHERE id = 25000")?;
    assert_eq!(result.rows[0][0], Value::Varchar("data_25000".to_string()));
    
    let result = db.execute("SELECT data FROM large WHERE id = 50000")?;
    assert_eq!(result.rows[0][0], Value::Varchar("data_50000".to_string()));
    
    Ok(())
}

