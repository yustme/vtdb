# VTDB - Snowflake-Compatible Database Engine

A high-performance embedded database engine written in Rust with 100% compatibility with Snowflake SQL dialect.

## Features

- **Snowflake SQL Dialect**: Full compatibility with Snowflake SQL syntax
- **Columnar Storage**: Optimized for analytical workloads
- **Vectorized Execution**: High-performance query processing
- **Hybrid Storage**: In-memory with optional disk persistence
- **Basic CRUD Operations**: CREATE, SELECT, INSERT, UPDATE, DELETE

## Usage

```rust
use vtdb::{Database, Result};

fn main() -> Result<()> {
    let mut db = Database::new();
    
    // Create a table
    db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)")?;
    
    // Insert data
    db.execute("INSERT INTO users VALUES (1, 'Alice')")?;
    
    // Query data
    let results = db.execute("SELECT name FROM users WHERE id = 1")?;
    
    Ok(())
}
```

## Architecture

- **Parser**: Snowflake SQL dialect parser
- **Planner**: Query planning and optimization
- **Storage**: Columnar storage engine with WAL
- **Execution**: Vectorized execution engine
- **Catalog**: Schema and metadata management

## Status

Early development - basic CRUD operations supported.

