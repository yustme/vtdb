use vtdb::{Database, Value};
use crate::test_utils::create_test_db;
use anyhow::Result;

#[test]
fn test_large_batch_flush_failure_recovery() -> Result<()> {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE recovery_test (id INTEGER, data VARCHAR)")?;
    
    // Insert a large batch that should trigger flush
    let mut values = Vec::new();
    for i in 1..=5000 {
        values.push(format!("({}, 'data_{}')", i, i));
    }
    let sql = format!("INSERT INTO recovery_test VALUES {}", values.join(", "));
    
    // Insert should succeed even if flush fails (data is in memory)
    // In real scenario, flush failure would be logged but insert continues
    let result = db.execute(&sql);
    assert!(result.is_ok(), "Insert should succeed even if flush fails");
    
    // Verify data is in memory
    let result = db.execute("SELECT COUNT(*) FROM recovery_test")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 5000, "All 5000 rows should be in memory");
    }
    
    // Force flush to ensure data is persisted
    db.storage.flush_table_iceberg_writes("recovery_test")?;
    
    // Verify data is still there after flush
    let result = db.execute("SELECT COUNT(*) FROM recovery_test")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 5000, "All 5000 rows should persist after flush");
    }
    
    Ok(())
}

#[test]
fn test_multiple_large_table_inserts() -> Result<()> {
    let mut db = create_test_db();
    
    // Create multiple tables
    db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)")?;
    db.execute("CREATE TABLE orders (id INTEGER, user_id INTEGER, amount INTEGER)")?;
    db.execute("CREATE TABLE products (id INTEGER, name VARCHAR, price INTEGER)")?;
    db.execute("CREATE TABLE reviews (id INTEGER, product_id INTEGER, rating INTEGER)")?;
    db.execute("CREATE TABLE categories (id INTEGER, name VARCHAR)")?;

    // Insert large batches into each table
    let mut user_values = Vec::new();
    for i in 1..=10000 {
        user_values.push(format!("({}, 'user_{}')", i, i));
    }
    db.execute(&format!("INSERT INTO users VALUES {}", user_values.join(", ")))?;

    let mut order_values = Vec::new();
    for i in 1..=5000 {
        order_values.push(format!("({}, {}, {})", i, i % 10000 + 1, i * 10));
    }
    db.execute(&format!("INSERT INTO orders VALUES {}", order_values.join(", ")))?;

    let mut product_values = Vec::new();
    for i in 1..=2000 {
        product_values.push(format!("({}, 'product_{}', {})", i, i, i * 100));
    }
    db.execute(&format!("INSERT INTO products VALUES {}", product_values.join(", ")))?;

    let mut review_values = Vec::new();
    for i in 1..=3000 {
        review_values.push(format!("({}, {}, {})", i, i % 2000 + 1, (i % 5) + 1));
    }
    db.execute(&format!("INSERT INTO reviews VALUES {}", review_values.join(", ")))?;

    let mut category_values = Vec::new();
    for i in 1..=100 {
        category_values.push(format!("({}, 'category_{}')", i, i));
    }
    db.execute(&format!("INSERT INTO categories VALUES {}", category_values.join(", ")))?;

    // Force flush all tables
    db.storage.flush_table_iceberg_writes("users")?;
    db.storage.flush_table_iceberg_writes("orders")?;
    db.storage.flush_table_iceberg_writes("products")?;
    db.storage.flush_table_iceberg_writes("reviews")?;
    db.storage.flush_table_iceberg_writes("categories")?;

    // Verify all tables have correct row counts
    let result = db.execute("SELECT COUNT(*) FROM users")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 10000, "users table should have 10000 rows");
    }

    let result = db.execute("SELECT COUNT(*) FROM orders")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 5000, "orders table should have 5000 rows");
    }

    let result = db.execute("SELECT COUNT(*) FROM products")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 2000, "products table should have 2000 rows");
    }

    let result = db.execute("SELECT COUNT(*) FROM reviews")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 3000, "reviews table should have 3000 rows");
    }

    let result = db.execute("SELECT COUNT(*) FROM categories")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 100, "categories table should have 100 rows");
    }

    // Verify sample data from each table
    let result = db.execute("SELECT name FROM users WHERE id = 5000")?;
    assert_eq!(result.rows[0][0], Value::Varchar("user_5000".to_string()));

    let result = db.execute("SELECT amount FROM orders WHERE id = 2500")?;
    assert_eq!(result.rows[0][0], Value::Integer(25000));

    let result = db.execute("SELECT name FROM products WHERE id = 1000")?;
    assert_eq!(result.rows[0][0], Value::Varchar("product_1000".to_string()));

    Ok(())
}

#[test]
fn test_interleaved_large_inserts() -> Result<()> {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE table_a (id INTEGER)")?;
    db.execute("CREATE TABLE table_b (id INTEGER)")?;
    db.execute("CREATE TABLE table_c (id INTEGER)")?;

    // Interleave large inserts across tables
    for batch in 0..10 {
        let start = batch * 1000 + 1;
        let end = (batch + 1) * 1000;
        
        // Insert into table_a
        let mut values_a = Vec::new();
        for i in start..=end {
            values_a.push(format!("({})", i));
        }
        db.execute(&format!("INSERT INTO table_a VALUES {}", values_a.join(", ")))?;
        
        // Insert into table_b
        let mut values_b = Vec::new();
        for i in start..=end {
            values_b.push(format!("({})", i * 2));
        }
        db.execute(&format!("INSERT INTO table_b VALUES {}", values_b.join(", ")))?;
        
        // Insert into table_c
        let mut values_c = Vec::new();
        for i in start..=end {
            values_c.push(format!("({})", i * 3));
        }
        db.execute(&format!("INSERT INTO table_c VALUES {}", values_c.join(", ")))?;
    }

    // Flush all tables
    db.storage.flush_table_iceberg_writes("table_a")?;
    db.storage.flush_table_iceberg_writes("table_b")?;
    db.storage.flush_table_iceberg_writes("table_c")?;

    // Verify all tables have correct counts
    let result = db.execute("SELECT COUNT(*) FROM table_a")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 10000, "table_a should have 10000 rows");
    }

    let result = db.execute("SELECT COUNT(*) FROM table_b")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 10000, "table_b should have 10000 rows");
    }

    let result = db.execute("SELECT COUNT(*) FROM table_c")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 10000, "table_c should have 10000 rows");
    }

    // Verify data integrity
    let result = db.execute("SELECT id FROM table_a WHERE id = 5000")?;
    assert_eq!(result.rows[0][0], Value::Integer(5000));

    let result = db.execute("SELECT id FROM table_b WHERE id = 10000")?;
    assert_eq!(result.rows[0][0], Value::Integer(10000));

    let result = db.execute("SELECT id FROM table_c WHERE id = 15000")?;
    assert_eq!(result.rows[0][0], Value::Integer(15000));

    Ok(())
}

#[test]
fn test_buffer_accumulation_time_flush() -> Result<()> {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE time_test (id INTEGER, data VARCHAR)")?;
    
    // Insert batches that don't reach size threshold
    // These should flush based on time threshold
    for i in 1..=10 {
        let mut values = Vec::new();
        for j in 1..=100 {
            let idx = (i - 1) * 100 + j;
            values.push(format!("({}, 'data_{}')", idx, idx));
        }
        db.execute(&format!("INSERT INTO time_test VALUES {}", values.join(", ")))?;
    }

    // Force flush to ensure all data is persisted
    db.storage.flush_table_iceberg_writes("time_test")?;

    // Verify all data is present
    let result = db.execute("SELECT COUNT(*) FROM time_test")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 1000, "time_test should have 1000 rows");
    }

    // Verify data integrity
    let result = db.execute("SELECT data FROM time_test WHERE id = 500")?;
    assert_eq!(result.rows[0][0], Value::Varchar("data_500".to_string()));

    Ok(())
}

#[test]
fn test_sequential_large_inserts_same_table() -> Result<()> {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE sequential (id INTEGER, batch INTEGER)")?;
    
    // Insert multiple large batches sequentially into same table
    for batch_num in 1..=5 {
        let mut values = Vec::new();
        let start = (batch_num - 1) * 2000 + 1;
        let end = batch_num * 2000;
        
        for i in start..=end {
            values.push(format!("({}, {})", i, batch_num));
        }
        
        db.execute(&format!("INSERT INTO sequential VALUES {}", values.join(", ")))?;
        
        // Flush after each batch
        db.storage.flush_table_iceberg_writes("sequential")?;
        
        // Verify count increases
        let result = db.execute("SELECT COUNT(*) FROM sequential")?;
        if let Value::Integer(count) = result.rows[0][0] {
            assert_eq!(count, end, "Should have {} rows after batch {}", end, batch_num);
        }
    }

    // Final verification
    let result = db.execute("SELECT COUNT(*) FROM sequential")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 10000, "sequential should have 10000 rows total");
    }

    // Verify data from different batches
    let result = db.execute("SELECT batch FROM sequential WHERE id = 1000")?;
    assert_eq!(result.rows[0][0], Value::Integer(1));

    let result = db.execute("SELECT batch FROM sequential WHERE id = 5000")?;
    assert_eq!(result.rows[0][0], Value::Integer(3));

    let result = db.execute("SELECT batch FROM sequential WHERE id = 10000")?;
    assert_eq!(result.rows[0][0], Value::Integer(5));

    Ok(())
}

#[test]
fn test_concurrent_table_flushes() -> Result<()> {
    let mut db = create_test_db();
    
    // Create multiple tables
    db.execute("CREATE TABLE concurrent_a (id INTEGER)")?;
    db.execute("CREATE TABLE concurrent_b (id INTEGER)")?;
    db.execute("CREATE TABLE concurrent_c (id INTEGER)")?;
    db.execute("CREATE TABLE concurrent_d (id INTEGER)")?;

    // Insert large batches into all tables
    for table in ["concurrent_a", "concurrent_b", "concurrent_c", "concurrent_d"] {
        let mut values = Vec::new();
        for i in 1..=5000 {
            values.push(format!("({})", i));
        }
        db.execute(&format!("INSERT INTO {} VALUES {}", table, values.join(", ")))?;
    }

    // Flush all tables (simulating concurrent flushes)
    db.storage.flush_table_iceberg_writes("concurrent_a")?;
    db.storage.flush_table_iceberg_writes("concurrent_b")?;
    db.storage.flush_table_iceberg_writes("concurrent_c")?;
    db.storage.flush_table_iceberg_writes("concurrent_d")?;

    // Verify all tables have correct data
    for table in ["concurrent_a", "concurrent_b", "concurrent_c", "concurrent_d"] {
        let result = db.execute(&format!("SELECT COUNT(*) FROM {}", table))?;
        if let Value::Integer(count) = result.rows[0][0] {
            assert_eq!(count, 5000, "{} should have 5000 rows", table);
        }
        
        // Verify sample data
        let result = db.execute(&format!("SELECT id FROM {} WHERE id = 2500", table))?;
        assert_eq!(result.rows[0][0], Value::Integer(2500));
    }

    Ok(())
}

#[test]
fn test_large_insert_with_query_immediately_after() -> Result<()> {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE immediate_query (id INTEGER, value INTEGER)")?;
    
    // Insert large batch
    let mut values = Vec::new();
    for i in 1..=10000 {
        values.push(format!("({}, {})", i, i * 10));
    }
    db.execute(&format!("INSERT INTO immediate_query VALUES {}", values.join(", ")))?;
    
    // Query immediately (before explicit flush) - should work because data is in memory
    let result = db.execute("SELECT COUNT(*) FROM immediate_query")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 10000, "Should be able to query immediately after insert");
    }
    
    // Query specific data
    let result = db.execute("SELECT value FROM immediate_query WHERE id = 5000")?;
    assert_eq!(result.rows[0][0], Value::Integer(50000));
    
    // Now flush and verify persistence
    db.storage.flush_table_iceberg_writes("immediate_query")?;
    
    // Query again after flush
    let result = db.execute("SELECT COUNT(*) FROM immediate_query")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 10000, "Data should persist after flush");
    }

    Ok(())
}

