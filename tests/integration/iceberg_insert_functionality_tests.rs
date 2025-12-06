use vtdb::{Database, Value};
use crate::test_utils::create_test_db;

#[test]
fn test_small_batch_insert_and_read() {
    let mut db = create_test_db();
    
    // Create table
    db.execute("CREATE TABLE small_test (id INTEGER, name VARCHAR)").unwrap();
    
    // Insert small batch (should flush immediately)
    db.execute("INSERT INTO small_test VALUES (1, 'Alice'), (2, 'Bob'), (3, 'Charlie')").unwrap();
    
    // Verify data is immediately visible
    let result = db.execute("SELECT COUNT(*) FROM small_test").unwrap();
    assert_eq!(result.rows.len(), 1);
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 3, "Should have 3 rows immediately after insert");
    }
    
    // Verify specific data
    let result = db.execute("SELECT name FROM small_test WHERE id = 1").unwrap();
    assert_eq!(result.rows.len(), 1);
    if let Value::Varchar(name) = &result.rows[0][0] {
        assert_eq!(name, "Alice");
    }
}

#[test]
fn test_multiple_small_inserts() {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE multi_insert (id INTEGER, value INTEGER)").unwrap();
    
    // Insert multiple small batches
    for i in 1..=10 {
        db.execute(&format!("INSERT INTO multi_insert VALUES ({}, {})", i, i * 10)).unwrap();
        
        // Verify data accumulates
        let result = db.execute("SELECT COUNT(*) FROM multi_insert").unwrap();
        if let Value::Integer(count) = result.rows[0][0] {
            assert_eq!(count, i, "Should have {} rows after {} inserts", i, i);
        }
    }
    
    // Verify final count
    let result = db.execute("SELECT COUNT(*) FROM multi_insert").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 10);
    }
    
    // Verify data integrity
    let result = db.execute("SELECT value FROM multi_insert WHERE id = 5").unwrap();
    assert_eq!(result.rows.len(), 1);
    if let Value::Integer(value) = result.rows[0][0] {
        assert_eq!(value, 50);
    }
}

#[test]
fn test_large_batch_insert() {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE large_test (id INTEGER, data VARCHAR)").unwrap();
    
    // Insert large batch (should be buffered)
    let mut values = Vec::new();
    for i in 1..=5000 {
        values.push(format!("({}, 'data_{}')", i, i));
    }
    let sql = format!("INSERT INTO large_test VALUES {}", values.join(", "));
    db.execute(&sql).unwrap();
    
    // Force flush
    db.storage.flush_table_iceberg_writes("large_test").unwrap();
    
    // Verify data
    let result = db.execute("SELECT COUNT(*) FROM large_test").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 5000, "Should have 5000 rows");
    }
    
    // Verify sample data
    let result = db.execute("SELECT data FROM large_test WHERE id = 2500").unwrap();
    assert_eq!(result.rows.len(), 1);
    if let Value::Varchar(data) = &result.rows[0][0] {
        assert_eq!(data, "data_2500");
    }
}

#[test]
fn test_multiple_tables_independent() {
    let mut db = create_test_db();
    
    // Create multiple tables
    db.execute("CREATE TABLE table_a (id INTEGER)").unwrap();
    db.execute("CREATE TABLE table_b (id INTEGER, name VARCHAR)").unwrap();
    db.execute("CREATE TABLE table_c (id INTEGER, value INTEGER)").unwrap();
    
    // Insert into each table
    db.execute("INSERT INTO table_a VALUES (1), (2), (3)").unwrap();
    db.execute("INSERT INTO table_b VALUES (10, 'ten'), (20, 'twenty')").unwrap();
    db.execute("INSERT INTO table_c VALUES (100, 1000), (200, 2000)").unwrap();
    
    // Force flush all
    db.storage.flush_table_iceberg_writes("table_a").unwrap();
    db.storage.flush_table_iceberg_writes("table_b").unwrap();
    db.storage.flush_table_iceberg_writes("table_c").unwrap();
    
    // Verify all tables have correct data
    let result = db.execute("SELECT COUNT(*) FROM table_a").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 3, "table_a should have 3 rows");
    }
    
    let result = db.execute("SELECT COUNT(*) FROM table_b").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 2, "table_b should have 2 rows");
    }
    
    let result = db.execute("SELECT COUNT(*) FROM table_c").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 2, "table_c should have 2 rows");
    }
    
    // Verify data integrity
    let result = db.execute("SELECT name FROM table_b WHERE id = 20").unwrap();
    assert_eq!(result.rows.len(), 1);
    if let Value::Varchar(name) = &result.rows[0][0] {
        assert_eq!(name, "twenty");
    }
}

#[test]
fn test_interleaved_inserts_multiple_tables() {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE x (id INTEGER)").unwrap();
    db.execute("CREATE TABLE y (id INTEGER)").unwrap();
    db.execute("CREATE TABLE z (id INTEGER)").unwrap();
    
    // Interleave inserts
    for i in 1..=30 {
        db.execute(&format!("INSERT INTO x VALUES ({})", i)).unwrap();
        db.execute(&format!("INSERT INTO y VALUES ({})", i * 2)).unwrap();
        db.execute(&format!("INSERT INTO z VALUES ({})", i * 3)).unwrap();
    }
    
    // Flush all
    db.storage.flush_table_iceberg_writes("x").unwrap();
    db.storage.flush_table_iceberg_writes("y").unwrap();
    db.storage.flush_table_iceberg_writes("z").unwrap();
    
    // Verify all tables
    let result = db.execute("SELECT COUNT(*) FROM x").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 30);
    }
    
    let result = db.execute("SELECT COUNT(*) FROM y").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 30);
    }
    
    let result = db.execute("SELECT COUNT(*) FROM z").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 30);
    }
    
    // Verify values
    let result = db.execute("SELECT id FROM x WHERE id = 15").unwrap();
    assert_eq!(result.rows[0][0], Value::Integer(15));
    
    let result = db.execute("SELECT id FROM y WHERE id = 30").unwrap();
    assert_eq!(result.rows[0][0], Value::Integer(30));
    
    let result = db.execute("SELECT id FROM z WHERE id = 45").unwrap();
    assert_eq!(result.rows[0][0], Value::Integer(45));
}

#[test]
fn test_data_persistence_after_flush() {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE persist_test (id INTEGER, data VARCHAR)").unwrap();
    
    // Insert data
    db.execute("INSERT INTO persist_test VALUES (1, 'first'), (2, 'second'), (3, 'third')").unwrap();
    
    // Flush to ensure data is written
    db.storage.flush_table_iceberg_writes("persist_test").unwrap();
    
    // Insert more data
    db.execute("INSERT INTO persist_test VALUES (4, 'fourth'), (5, 'fifth')").unwrap();
    
    // Flush again
    db.storage.flush_table_iceberg_writes("persist_test").unwrap();
    
    // Verify all data is present
    let result = db.execute("SELECT COUNT(*) FROM persist_test").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 5, "Should have all 5 rows after multiple flushes");
    }
    
    // Verify all values
    let result = db.execute("SELECT data FROM persist_test ORDER BY id").unwrap();
    assert_eq!(result.rows.len(), 5);
    assert_eq!(result.rows[0][0], Value::Varchar("first".to_string()));
    assert_eq!(result.rows[1][0], Value::Varchar("second".to_string()));
    assert_eq!(result.rows[2][0], Value::Varchar("third".to_string()));
    assert_eq!(result.rows[3][0], Value::Varchar("fourth".to_string()));
    assert_eq!(result.rows[4][0], Value::Varchar("fifth".to_string()));
}

#[test]
fn test_customers_table_large_insert() {
    let mut db = create_test_db();
    
    // Simulate CUSTOMERS table scenario
    db.execute("CREATE TABLE CUSTOMERS (id INTEGER, name VARCHAR, email VARCHAR)").unwrap();
    
    // Insert 5000 rows (as mentioned by user)
    let mut values = Vec::new();
    for i in 1..=5000 {
        values.push(format!("({}, 'Customer_{}', 'customer_{}@example.com')", i, i, i));
    }
    
    // Insert in batches to test batching
    let batch_size = 100;
    for batch_start in (0..5000).step_by(batch_size) {
        let batch_values: Vec<String> = values[batch_start..(batch_start + batch_size).min(5000)]
            .iter()
            .cloned()
            .collect();
        let sql = format!("INSERT INTO CUSTOMERS VALUES {}", batch_values.join(", "));
        db.execute(&sql).unwrap();
    }
    
    // Force flush
    db.storage.flush_table_iceberg_writes("CUSTOMERS").unwrap();
    
    // Verify count
    let result = db.execute("SELECT COUNT(*) FROM CUSTOMERS").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 5000, "CUSTOMERS table should have 5000 rows");
    }
    
    // Verify sample data
    let result = db.execute("SELECT name FROM CUSTOMERS WHERE id = 2500").unwrap();
    assert_eq!(result.rows.len(), 1);
    if let Value::Varchar(name) = &result.rows[0][0] {
        assert_eq!(name, "Customer_2500");
    }
    
    // Verify range query
    let result = db.execute("SELECT COUNT(*) FROM CUSTOMERS WHERE id BETWEEN 100 AND 200").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 101); // 100 to 200 inclusive
    }
}

#[test]
fn test_empty_table_after_insert() {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE empty_test (id INTEGER)").unwrap();
    
    // Insert data
    db.execute("INSERT INTO empty_test VALUES (1), (2), (3)").unwrap();
    
    // Immediately check - should not be empty
    let result = db.execute("SELECT COUNT(*) FROM empty_test").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert!(count > 0, "Table should not be empty after insert");
        assert_eq!(count, 3);
    }
    
    // Flush and check again
    db.storage.flush_table_iceberg_writes("empty_test").unwrap();
    
    let result = db.execute("SELECT COUNT(*) FROM empty_test").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 3, "Table should still have 3 rows after flush");
    }
}

#[test]
fn test_single_row_insert() {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE single (id INTEGER, name VARCHAR)").unwrap();
    
    // Insert single row (smallest possible batch)
    db.execute("INSERT INTO single VALUES (1, 'one')").unwrap();
    
    // Should be immediately visible
    let result = db.execute("SELECT COUNT(*) FROM single").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 1);
    }
    
    // Insert more single rows
    db.execute("INSERT INTO single VALUES (2, 'two')").unwrap();
    db.execute("INSERT INTO single VALUES (3, 'three')").unwrap();
    
    // Verify all are present
    let result = db.execute("SELECT COUNT(*) FROM single").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 3);
    }
}

#[test]
fn test_mixed_data_types() {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE mixed (id INTEGER, name VARCHAR, active BOOLEAN, score INTEGER)").unwrap();
    
    db.execute("INSERT INTO mixed VALUES (1, 'Alice', true, 100), (2, 'Bob', false, 200), (3, 'Charlie', true, 300)").unwrap();
    
    // Flush
    db.storage.flush_table_iceberg_writes("mixed").unwrap();
    
    // Verify count
    let result = db.execute("SELECT COUNT(*) FROM mixed").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 3);
    }
    
    // Verify boolean values
    let result = db.execute("SELECT active FROM mixed WHERE id = 1").unwrap();
    assert_eq!(result.rows[0][0], Value::Boolean(true));
    
    let result = db.execute("SELECT active FROM mixed WHERE id = 2").unwrap();
    assert_eq!(result.rows[0][0], Value::Boolean(false));
    
    // Verify all columns
    let result = db.execute("SELECT * FROM mixed WHERE id = 3").unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], Value::Integer(3));
    assert_eq!(result.rows[0][1], Value::Varchar("Charlie".to_string()));
    assert_eq!(result.rows[0][2], Value::Boolean(true));
    assert_eq!(result.rows[0][3], Value::Integer(300));
}

#[test]
fn test_sequential_flushes() {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE sequential (id INTEGER)").unwrap();
    
    // Insert and flush multiple times
    for batch in 1..=5 {
        let start = (batch - 1) * 10 + 1;
        let end = batch * 10;
        let mut values = Vec::new();
        for i in start..=end {
            values.push(format!("({})", i));
        }
        let sql = format!("INSERT INTO sequential VALUES {}", values.join(", "));
        db.execute(&sql).unwrap();
        
        // Flush after each batch
        db.storage.flush_table_iceberg_writes("sequential").unwrap();
        
        // Verify count increases
        let result = db.execute("SELECT COUNT(*) FROM sequential").unwrap();
        if let Value::Integer(count) = result.rows[0][0] {
            assert_eq!(count, end, "Should have {} rows after batch {}", end, batch);
        }
    }
    
    // Final verification
    let result = db.execute("SELECT COUNT(*) FROM sequential").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 50);
    }
}

#[test]
fn test_table_isolation_after_multiple_inserts() {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE isolated_a (id INTEGER)").unwrap();
    db.execute("CREATE TABLE isolated_b (id INTEGER)").unwrap();
    
    // Insert into A
    db.execute("INSERT INTO isolated_a VALUES (1), (2), (3)").unwrap();
    db.storage.flush_table_iceberg_writes("isolated_a").unwrap();
    
    // Insert into B
    db.execute("INSERT INTO isolated_b VALUES (10), (20), (30)").unwrap();
    db.storage.flush_table_iceberg_writes("isolated_b").unwrap();
    
    // Insert more into A
    db.execute("INSERT INTO isolated_a VALUES (4), (5)").unwrap();
    db.storage.flush_table_iceberg_writes("isolated_a").unwrap();
    
    // Verify A still has correct data
    let result = db.execute("SELECT COUNT(*) FROM isolated_a").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 5, "Table A should have 5 rows");
    }
    
    // Verify B still has correct data
    let result = db.execute("SELECT COUNT(*) FROM isolated_b").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 3, "Table B should have 3 rows");
    }
    
    // Verify values
    let result = db.execute("SELECT id FROM isolated_a ORDER BY id").unwrap();
    assert_eq!(result.rows.len(), 5);
    assert_eq!(result.rows[0][0], Value::Integer(1));
    assert_eq!(result.rows[4][0], Value::Integer(5));
    
    let result = db.execute("SELECT id FROM isolated_b ORDER BY id").unwrap();
    assert_eq!(result.rows.len(), 3);
    assert_eq!(result.rows[0][0], Value::Integer(10));
    assert_eq!(result.rows[2][0], Value::Integer(30));
}

#[test]
fn test_partial_column_insert_two_columns() {
    let mut db = create_test_db();
    
    // Create table with 2 columns
    db.execute("CREATE TABLE partial_test (id INTEGER, name VARCHAR)").unwrap();
    
    // Insert only 1 value - should pad with NULL
    db.execute("INSERT INTO partial_test VALUES (1)").unwrap();
    
    // Verify the row was inserted with NULL for the second column
    let result = db.execute("SELECT * FROM partial_test").unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], Value::Integer(1));
    assert_eq!(result.rows[0][1], Value::Null);
    
    // Verify we can query with NULL
    let result = db.execute("SELECT name FROM partial_test WHERE id = 1").unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], Value::Null);
}

#[test]
fn test_partial_column_insert_three_columns() {
    let mut db = create_test_db();
    
    // Create table with 3 columns
    db.execute("CREATE TABLE three_col_test (id INTEGER, name VARCHAR, score INTEGER)").unwrap();
    
    // Insert only 2 values - should pad with NULL for the third column
    db.execute("INSERT INTO three_col_test VALUES (1, 'Alice')").unwrap();
    
    // Verify the row was inserted with NULL for the third column
    let result = db.execute("SELECT * FROM three_col_test").unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], Value::Integer(1));
    assert_eq!(result.rows[0][1], Value::Varchar("Alice".to_string()));
    assert_eq!(result.rows[0][2], Value::Null);
    
    // Insert only 1 value - should pad with NULL for second and third columns
    db.execute("INSERT INTO three_col_test VALUES (2)").unwrap();
    
    let result = db.execute("SELECT * FROM three_col_test WHERE id = 2").unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], Value::Integer(2));
    assert_eq!(result.rows[0][1], Value::Null);
    assert_eq!(result.rows[0][2], Value::Null);
}

#[test]
fn test_explicit_null_values_still_work() {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE null_test (id INTEGER, name VARCHAR)").unwrap();
    
    // Explicit NULL values should still work
    db.execute("INSERT INTO null_test VALUES (1, NULL)").unwrap();
    
    let result = db.execute("SELECT * FROM null_test").unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], Value::Integer(1));
    assert_eq!(result.rows[0][1], Value::Null);
    
    // Insert with first column NULL (should work if we support it)
    db.execute("INSERT INTO null_test VALUES (NULL, 'test')").unwrap();
    
    let result = db.execute("SELECT * FROM null_test WHERE name = 'test'").unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], Value::Null);
    assert_eq!(result.rows[0][1], Value::Varchar("test".to_string()));
}

#[test]
fn test_full_inserts_still_work() {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE full_test (id INTEGER, name VARCHAR)").unwrap();
    
    // Full inserts should continue to work as before
    db.execute("INSERT INTO full_test VALUES (1, 'Alice')").unwrap();
    
    let result = db.execute("SELECT * FROM full_test").unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], Value::Integer(1));
    assert_eq!(result.rows[0][1], Value::Varchar("Alice".to_string()));
    
    // Multiple full inserts
    db.execute("INSERT INTO full_test VALUES (2, 'Bob'), (3, 'Charlie')").unwrap();
    
    let result = db.execute("SELECT COUNT(*) FROM full_test").unwrap();
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 3);
    }
}

#[test]
fn test_partial_inserts_with_multiple_rows() {
    let mut db = create_test_db();
    
    db.execute("CREATE TABLE multi_partial (id INTEGER, name VARCHAR, active BOOLEAN)").unwrap();
    
    // Insert multiple rows with partial values
    db.execute("INSERT INTO multi_partial VALUES (1), (2, 'Bob'), (3, 'Charlie', true)").unwrap();
    
    let result = db.execute("SELECT * FROM multi_partial ORDER BY id").unwrap();
    assert_eq!(result.rows.len(), 3);
    
    // First row: (1, NULL, NULL)
    assert_eq!(result.rows[0][0], Value::Integer(1));
    assert_eq!(result.rows[0][1], Value::Null);
    assert_eq!(result.rows[0][2], Value::Null);
    
    // Second row: (2, 'Bob', NULL)
    assert_eq!(result.rows[1][0], Value::Integer(2));
    assert_eq!(result.rows[1][1], Value::Varchar("Bob".to_string()));
    assert_eq!(result.rows[1][2], Value::Null);
    
    // Third row: (3, 'Charlie', true) - full insert
    assert_eq!(result.rows[2][0], Value::Integer(3));
    assert_eq!(result.rows[2][1], Value::Varchar("Charlie".to_string()));
    assert_eq!(result.rows[2][2], Value::Boolean(true));
}

