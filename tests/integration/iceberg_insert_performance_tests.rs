use anyhow::Result;
use std::time::Instant;
use vtdb::Database;
use vtdb::Value;

fn create_test_database() -> (Database, tempfile::TempDir) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let iceberg_path = temp_dir.path().join("iceberg");
    std::fs::create_dir_all(&iceberg_path).unwrap();
    
    use vtdb::storage::StorageEngine;
    use vtdb::catalog::Catalog;
    use vtdb::planner::Planner;
    use vtdb::execution::Executor;
    use vtdb::transaction::TransactionManager;
    use vtdb::cache::QueryCache;
    use std::sync::{Arc, Mutex};
    use std::collections::HashMap;
    
    let mut catalog = Catalog::new();
    let mut storage = StorageEngine::with_iceberg_path(&iceberg_path).unwrap();
    let _ = Database::load_tables_from_storage(&mut catalog, &mut storage);

    let db = Database {
        catalog,
        storage,
        planner: Planner::new(),
        executor: Executor::new(),
        transaction_manager: TransactionManager::new(),
        query_cache: QueryCache::new(),
        query_progress: Arc::new(Mutex::new(HashMap::new())),
    };
    
    (db, temp_dir)
}

#[test]
fn test_iceberg_write_buffering() -> Result<()> {
    let (mut db, _temp_dir) = create_test_database();
    
    // Create table
    db.execute("CREATE TABLE test_table (id INTEGER, name VARCHAR)")?;

    // Insert small batches - should be buffered
    for i in 0..100 {
        db.execute(&format!("INSERT INTO test_table VALUES ({}, 'name_{}')", i, i))?;
    }

    // Force flush to verify data is written
    db.storage.flush_table_iceberg_writes("test_table")?;

    // Verify data can be read back
    let result = db.execute("SELECT COUNT(*) FROM test_table")?;
    assert_eq!(result.rows.len(), 1);
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 100);
    }

    Ok(())
}

#[test]
fn test_iceberg_large_insert_performance() -> Result<()> {
    let (mut db, _temp_dir) = create_test_database();
    
    // Create table
    db.execute("CREATE TABLE large_table (id INTEGER, value INTEGER)")?;

    // Configure for optimal performance
    db.storage.configure_iceberg_writes(|config| {
        config.target_file_size_bytes = 64 * 1024 * 1024; // 64MB for testing
        config.manifest_batch_size = 5;
        config.snapshot_interval_files = 5;
        config.write_parallelism = 2;
    });

    // Insert 10K rows in batches using SQL
    let start = Instant::now();
    let batch_size = 1000;
    let total_rows = 10_000;

    for batch_start in (0..total_rows).step_by(batch_size) {
        let mut values = Vec::new();
        for i in batch_start..(batch_start + batch_size).min(total_rows) {
            values.push(format!("({}, {})", i, i * 2));
        }
        let sql = format!("INSERT INTO large_table VALUES {}", values.join(", "));
        db.execute(&sql)?;
    }

    // Force flush
    db.storage.flush_table_iceberg_writes("large_table")?;
    let elapsed = start.elapsed();

    println!("Inserted {} rows in {:?}", total_rows, elapsed);
    println!("Rate: {:.2} rows/second", total_rows as f64 / elapsed.as_secs_f64());

    // Verify data integrity
    let result = db.execute("SELECT COUNT(*) FROM large_table")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, total_rows as i64);
    }

    // Verify sample data
    let result = db.execute("SELECT id FROM large_table WHERE id = 0")?;
    assert_eq!(result.rows.len(), 1);

    Ok(())
}

#[test]
fn test_iceberg_manifest_batching() -> Result<()> {
    let (mut db, _temp_dir) = create_test_database();
    
    db.execute("CREATE TABLE manifest_test (id INTEGER)")?;

    // Configure small manifest batch size to test batching
    db.storage.configure_iceberg_writes(|config| {
        config.target_file_size_bytes = 1024 * 1024; // 1MB - small to trigger multiple files
        config.manifest_batch_size = 3; // Batch 3 files per manifest
        config.snapshot_interval_files = 10; // Don't create snapshots too often
    });

    // Insert enough data to create multiple files
    let rows_per_batch = 10_000;
    for i in 0..5 {
        let mut values = Vec::new();
        for j in 0..rows_per_batch {
            values.push(format!("({})", i * rows_per_batch + j));
        }
        let sql = format!("INSERT INTO manifest_test VALUES {}", values.join(", "));
        db.execute(&sql)?;
    }

    // Force flush
    db.storage.flush_table_iceberg_writes("manifest_test")?;

    // Verify data
    let result = db.execute("SELECT COUNT(*) FROM manifest_test")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, (5 * rows_per_batch) as i64);
    }

    Ok(())
}

#[test]
fn test_iceberg_snapshot_batching() -> Result<()> {
    let (mut db, _temp_dir) = create_test_database();
    
    db.execute("CREATE TABLE snapshot_test (id INTEGER)")?;

    // Configure snapshot batching
    db.storage.configure_iceberg_writes(|config| {
        config.target_file_size_bytes = 1024 * 1024; // 1MB
        config.manifest_batch_size = 2;
        config.snapshot_interval_files = 3; // Create snapshot every 3 files
    });

    // Insert data to trigger multiple snapshots
    let rows_per_batch = 10_000;
    for i in 0..10 {
        let mut values = Vec::new();
        for j in 0..rows_per_batch {
            values.push(format!("({})", i * rows_per_batch + j));
        }
        let sql = format!("INSERT INTO snapshot_test VALUES {}", values.join(", "));
        db.execute(&sql)?;
    }

    // Force flush to create final snapshot
    db.storage.flush_table_iceberg_writes("snapshot_test")?;

    // Verify data integrity
    let result = db.execute("SELECT COUNT(*) FROM snapshot_test")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, (10 * rows_per_batch) as i64);
    }

    Ok(())
}

#[test]
fn test_iceberg_parallel_writes() -> Result<()> {
    let (mut db, _temp_dir) = create_test_database();
    
    db.execute("CREATE TABLE parallel_test (id INTEGER, data VARCHAR)")?;

    // Configure for parallel writes
    db.storage.configure_iceberg_writes(|config| {
        config.target_file_size_bytes = 2 * 1024 * 1024; // 2MB - will trigger parallel writes
        config.write_parallelism = 4;
        config.estimated_bytes_per_row = 100; // Estimate to trigger splitting
    });

    // Insert large batch that will be split and written in parallel
    let start = Instant::now();
    let mut values = Vec::new();
    for i in 0..50_000 {
        values.push(format!("({}, 'data_{}')", i, i));
    }
    let sql = format!("INSERT INTO parallel_test VALUES {}", values.join(", "));
    db.execute(&sql)?;
    db.storage.flush_table_iceberg_writes("parallel_test")?;
    let elapsed = start.elapsed();

    println!("Parallel write test completed in {:?}", elapsed);

    // Verify data
    let result = db.execute("SELECT COUNT(*) FROM parallel_test")?;
    if let Value::Integer(count) = result.rows[0][0] {
        assert_eq!(count, 50_000);
    }

    Ok(())
}

#[test]
fn test_iceberg_configuration() -> Result<()> {
    let (mut db, _temp_dir) = create_test_database();
    
    // Test default configuration
    let config = db.storage.iceberg_write_config();
    assert_eq!(config.target_file_size_bytes, 128 * 1024 * 1024);
    assert_eq!(config.manifest_batch_size, 10);
    assert_eq!(config.parquet_compression, "snappy");

    // Test custom configuration
    db.storage.configure_iceberg_writes(|config| {
        config.target_file_size_bytes = 256 * 1024 * 1024;
        config.manifest_batch_size = 20;
        config.parquet_compression = "zstd".to_string();
    });

    let updated_config = db.storage.iceberg_write_config();
    assert_eq!(updated_config.target_file_size_bytes, 256 * 1024 * 1024);
    assert_eq!(updated_config.manifest_batch_size, 20);
    assert_eq!(updated_config.parquet_compression, "zstd");

    Ok(())
}

