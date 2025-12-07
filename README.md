# VTDB - Snowflake-Compatible Database Engine

A high-performance embedded database engine written in Rust with 100% compatibility with Snowflake SQL dialect.

## Features

- **Snowflake SQL Dialect**: Full compatibility with Snowflake SQL syntax
- **Columnar Storage**: Optimized for analytical workloads
- **Vectorized Execution**: High-performance query processing
- **Hybrid Storage**: In-memory with optional disk persistence
- **Basic CRUD Operations**: CREATE, SELECT, INSERT, UPDATE, DELETE

## Quick Start

### Using Docker (Recommended)

The easiest way to run VTDB is using Docker Compose:

```bash
# Build and start the database server
docker-compose up --build

# Or run in detached mode
docker-compose up -d --build

# View logs
docker-compose logs -f

# Stop the server
docker-compose down
```

The web interface will be available at `http://localhost:8080`. Data is persisted in the `./data` directory.

### Running Locally

If you have Rust installed, you can run the server directly:

```bash
# Build and run the server
./start-server.sh

# Or manually
cargo build --release
cargo run --release
```

The server will start on `http://localhost:8080`.

### Using as a Rust Library

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

## Web Interface

VTDB includes a web-based SQL query interface accessible at `http://localhost:8080` when the server is running. The interface provides:

- Interactive SQL query execution
- Table schema browsing
- Query result visualization
- Table management

## Requirements

- **For Docker**: Docker and Docker Compose
- **For Local Development**: Rust 1.75+ and Cargo

## Status

Early development - basic CRUD operations supported.

