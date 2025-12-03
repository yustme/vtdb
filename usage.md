# VTDB Usage Guide

This guide explains how to use VTDB (Snowflake-Compatible Database Engine) from external applications and how to generate and load sample datasets.

## Table of Contents

1. [REST API Usage](#rest-api-usage)
2. [Library API Usage (Rust)](#library-api-usage-rust)
3. [SQL Syntax Reference](#sql-syntax-reference)
4. [Data Types](#data-types)
5. [Sample Dataset Generation](#sample-dataset-generation)
6. [Loading Data](#loading-data)
7. [Performance Optimizations](#performance-optimizations)

---

## REST API Usage

VTDB provides a REST API endpoint for executing SQL queries. The server runs on `http://localhost:8080` by default.

### Starting the Server

```bash
cargo run --bin vtdb
```

The server will start on `http://localhost:8080` and automatically open a web browser with the SQL query interface.

### API Endpoints

#### Execute Query

**Endpoint:** `POST /api/execute`

**Request Format:**
```json
{
  "query": "SQL statement here"
}
```

**Response Format:**
```json
{
  "success": true,
  "result": {
    "rows": [
      [value1, value2, ...],
      [value1, value2, ...]
    ],
    "columns": ["column1", "column2", ...]
  },
  "error": null
}
```

**Error Response:**
```json
{
  "success": false,
  "result": null,
  "error": "Error message here"
}
```

#### List Tables

**Endpoint:** `GET /api/tables`

**Response Format:**
```json
{
  "success": true,
  "tables": ["table1", "table2", "table3"],
  "error": null
}
```

Returns a list of all table names in the database. Used by the web interface table explorer.

### Example: Using cURL

```bash
# Create a table
curl -X POST http://localhost:8080/api/execute \
  -H "Content-Type: application/json" \
  -d '{"query": "CREATE TABLE users (id INTEGER, name VARCHAR, email VARCHAR)"}'

# Insert data
curl -X POST http://localhost:8080/api/execute \
  -H "Content-Type: application/json" \
  -d '{"query": "INSERT INTO users VALUES (1, '\''Alice'\'', '\''alice@example.com'\'')"}'

# Query data
curl -X POST http://localhost:8080/api/execute \
  -H "Content-Type: application/json" \
  -d '{"query": "SELECT * FROM users WHERE id = 1"}'

# List all tables
curl -X GET http://localhost:8080/api/tables
```

### Example: Using Python

```python
import requests
import json

BASE_URL = "http://localhost:8080/api/execute"

def execute_query(query):
    response = requests.post(
        BASE_URL,
        json={"query": query},
        headers={"Content-Type": "application/json"}
    )
    return response.json()

# Create table
result = execute_query("CREATE TABLE users (id INTEGER, name VARCHAR, email VARCHAR)")
print(result)

# Insert data
result = execute_query("INSERT INTO users VALUES (1, 'Alice', 'alice@example.com')")
print(result)

# Query data
result = execute_query("SELECT * FROM users WHERE id = 1")
print(result)
```

### Example: Using JavaScript/Node.js

```javascript
const fetch = require('node-fetch');

const BASE_URL = 'http://localhost:8080/api/execute';

async function executeQuery(query) {
    const response = await fetch(BASE_URL, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query })
    });
    return await response.json();
}

// Create table
executeQuery("CREATE TABLE users (id INTEGER, name VARCHAR, email VARCHAR)")
    .then(result => console.log(result));

// Insert data
executeQuery("INSERT INTO users VALUES (1, 'Alice', 'alice@example.com')")
    .then(result => console.log(result));

// Query data
executeQuery("SELECT * FROM users WHERE id = 1")
    .then(result => console.log(result));
```

---

## Library API Usage (Rust)

If you're using VTDB as a Rust library, you can use the programmatic API:

```rust
use vtdb::Database;

fn main() -> anyhow::Result<()> {
    // Create a new database instance
    let mut db = Database::new();
    
    // Execute SQL queries
    let result = db.execute("CREATE TABLE users (id INTEGER, name VARCHAR)")?;
    
    let result = db.execute("INSERT INTO users VALUES (1, 'Alice')")?;
    
    let result = db.execute("SELECT * FROM users WHERE id = 1")?;
    
    // Access results
    println!("Columns: {:?}", result.columns);
    println!("Rows: {:?}", result.rows);
    
    Ok(())
}
```

### QueryResult Structure

```rust
pub struct QueryResult {
    pub rows: Vec<Vec<Value>>,      // Data rows
    pub columns: Vec<String>,       // Column names
}

pub enum Value {
    Integer(i64),
    Varchar(String),
    Boolean(bool),
    Null,
}
```

---

## SQL Syntax Reference

VTDB supports Snowflake-compatible SQL syntax for basic operations.

### CREATE TABLE

```sql
CREATE TABLE table_name (
    column1 INTEGER,
    column2 VARCHAR,
    column3 BOOLEAN
)
```

**Example:**
```sql
CREATE TABLE products (
    id INTEGER,
    name VARCHAR,
    price INTEGER,
    in_stock BOOLEAN
)
```

### INSERT

```sql
INSERT INTO table_name VALUES (value1, value2, value3)
```

**Example:**
```sql
INSERT INTO products VALUES (1, 'Laptop', 999, true)
INSERT INTO products VALUES (2, 'Mouse', 29, false)
```

**Performance Note:** VTDB uses optimized batch insertion internally. When inserting multiple rows, each INSERT statement is processed efficiently using columnar batch insertion with pre-allocated memory. This provides excellent performance for bulk data loading operations.

### SELECT

```sql
SELECT column1, column2 FROM table_name WHERE condition
SELECT * FROM table_name WHERE condition
```

**Examples:**
```sql
-- Select all columns
SELECT * FROM products

-- Select specific columns
SELECT name, price FROM products

-- Select with WHERE clause
SELECT * FROM products WHERE price > 100
SELECT * FROM products WHERE name = 'Laptop' AND in_stock = true
SELECT * FROM products WHERE price < 50 OR in_stock = false
```

**Supported WHERE operators:**
- `=` (equals)
- `!=` (not equals)
- `<` (less than)
- `>` (greater than)
- `<=` (less than or equal)
- `>=` (greater than or equal)
- `AND` (logical AND)
- `OR` (logical OR)

### UPDATE

```sql
UPDATE table_name SET column1 = value1, column2 = value2 WHERE condition
```

**Example:**
```sql
UPDATE products SET price = 899 WHERE id = 1
UPDATE products SET in_stock = true WHERE name = 'Mouse'
```

### DELETE

```sql
DELETE FROM table_name WHERE condition
```

**Example:**
```sql
DELETE FROM products WHERE id = 1
DELETE FROM products WHERE price < 10
```

---

## Data Types

VTDB supports the following data types:

| Type | Description | Example Values |
|------|-------------|----------------|
| `INTEGER` | 64-bit signed integer | `1`, `-42`, `1000` |
| `VARCHAR` | Variable-length string | `'Hello'`, `"World"` |
| `BOOLEAN` | Boolean value | `true`, `false` |

**Note:** String literals can be enclosed in single quotes (`'text'`) or double quotes (`"text"`).

---

## Sample Dataset Generation

This section provides examples for generating sample datasets that can be loaded into VTDB.

### Example 1: E-Commerce Products Dataset

**Table Schema:**
```sql
CREATE TABLE products (
    id INTEGER,
    name VARCHAR,
    category VARCHAR,
    price INTEGER,
    stock_quantity INTEGER,
    in_stock BOOLEAN
)
```

**Sample Data Generation (Python):**
```python
import random

products = [
    ("Laptop", "Electronics", 999),
    ("Mouse", "Electronics", 29),
    ("Keyboard", "Electronics", 79),
    ("Monitor", "Electronics", 299),
    ("Desk Chair", "Furniture", 199),
    ("Desk", "Furniture", 299),
    ("Lamp", "Furniture", 49),
    ("Notebook", "Office Supplies", 5),
    ("Pen", "Office Supplies", 2),
    ("Stapler", "Office Supplies", 15),
]

def generate_inserts(table_name, products):
    inserts = []
    for i, (name, category, price) in enumerate(products, 1):
        stock = random.randint(0, 100)
        in_stock = stock > 0
        insert = f"INSERT INTO {table_name} VALUES ({i}, '{name}', '{category}', {price}, {stock}, {in_stock})"
        inserts.append(insert)
    return inserts

# Generate SQL INSERT statements
inserts = generate_inserts("products", products)
for insert in inserts:
    print(insert)
```

**Generated SQL:**
```sql
INSERT INTO products VALUES (1, 'Laptop', 'Electronics', 999, 45, true)
INSERT INTO products VALUES (2, 'Mouse', 'Electronics', 29, 0, false)
INSERT INTO products VALUES (3, 'Keyboard', 'Electronics', 79, 23, true)
-- ... more rows
```

### Example 2: User Management Dataset

**Table Schema:**
```sql
CREATE TABLE users (
    id INTEGER,
    username VARCHAR,
    email VARCHAR,
    age INTEGER,
    active BOOLEAN
)
```

**Sample Data Generation (JavaScript):**
```javascript
const names = ['Alice', 'Bob', 'Charlie', 'Diana', 'Eve', 'Frank', 'Grace', 'Henry'];
const domains = ['example.com', 'test.com', 'demo.org'];

function generateUsers(count) {
    const inserts = [];
    for (let i = 1; i <= count; i++) {
        const name = names[Math.floor(Math.random() * names.length)];
        const email = `${name.toLowerCase()}@${domains[Math.floor(Math.random() * domains.length)]}`;
        const age = Math.floor(Math.random() * 50) + 18;
        const active = Math.random() > 0.3;
        inserts.push(`INSERT INTO users VALUES (${i}, '${name}', '${email}', ${age}, ${active})`);
    }
    return inserts;
}

const inserts = generateUsers(20);
inserts.forEach(insert => console.log(insert));
```

### Example 3: Sales Transactions Dataset

**Table Schema:**
```sql
CREATE TABLE sales (
    id INTEGER,
    product_id INTEGER,
    customer_id INTEGER,
    quantity INTEGER,
    total_amount INTEGER,
    sale_date VARCHAR
)
```

**Sample Data Generation (Python):**
```python
import random
from datetime import datetime, timedelta

def generate_sales(count, product_count, customer_count):
    inserts = []
    start_date = datetime(2024, 1, 1)
    
    for i in range(1, count + 1):
        product_id = random.randint(1, product_count)
        customer_id = random.randint(1, customer_count)
        quantity = random.randint(1, 10)
        unit_price = random.randint(10, 500)
        total_amount = quantity * unit_price
        
        days_offset = random.randint(0, 365)
        sale_date = (start_date + timedelta(days=days_offset)).strftime('%Y-%m-%d')
        
        insert = f"INSERT INTO sales VALUES ({i}, {product_id}, {customer_id}, {quantity}, {total_amount}, '{sale_date}')"
        inserts.append(insert)
    
    return inserts

inserts = generate_sales(100, 10, 50)
for insert in inserts:
    print(insert)
```

---

## Loading Data

### Method 1: Using REST API (Recommended for External Applications)

**Python Script:**
```python
import requests
import json

BASE_URL = "http://localhost:8080/api/execute"

def execute_query(query):
    response = requests.post(
        BASE_URL,
        json={"query": query},
        headers={"Content-Type": "application/json"}
    )
    result = response.json()
    if not result["success"]:
        raise Exception(f"Query failed: {result.get('error')}")
    return result

# Create table
execute_query("""
    CREATE TABLE products (
        id INTEGER,
        name VARCHAR,
        category VARCHAR,
        price INTEGER,
        stock_quantity INTEGER,
        in_stock BOOLEAN
    )
""")

# Load data
data = [
    (1, "Laptop", "Electronics", 999, 45, True),
    (2, "Mouse", "Electronics", 29, 0, False),
    (3, "Keyboard", "Electronics", 79, 23, True),
]

for row in data:
    id_val, name, category, price, stock, in_stock = row
    query = f"INSERT INTO products VALUES ({id_val}, '{name}', '{category}', {price}, {stock}, {in_stock})"
    execute_query(query)
    print(f"Inserted: {name}")
```

**Bash Script:**
```bash
#!/bin/bash

BASE_URL="http://localhost:8080/api/execute"

execute_query() {
    curl -X POST "$BASE_URL" \
        -H "Content-Type: application/json" \
        -d "{\"query\": \"$1\"}" | jq .
}

# Create table
execute_query "CREATE TABLE products (id INTEGER, name VARCHAR, category VARCHAR, price INTEGER, stock_quantity INTEGER, in_stock BOOLEAN)"

# Insert data
execute_query "INSERT INTO products VALUES (1, 'Laptop', 'Electronics', 999, 45, true)"
execute_query "INSERT INTO products VALUES (2, 'Mouse', 'Electronics', 29, 0, false)"
execute_query "INSERT INTO products VALUES (3, 'Keyboard', 'Electronics', 79, 23, true)"
```

### Method 2: Using Rust Library

```rust
use vtdb::Database;

fn main() -> anyhow::Result<()> {
    let mut db = Database::new();
    
    // Create table
    db.execute("CREATE TABLE products (id INTEGER, name VARCHAR, category VARCHAR, price INTEGER, stock_quantity INTEGER, in_stock BOOLEAN)")?;
    
    // Insert data
    let inserts = vec![
        "INSERT INTO products VALUES (1, 'Laptop', 'Electronics', 999, 45, true)",
        "INSERT INTO products VALUES (2, 'Mouse', 'Electronics', 29, 0, false)",
        "INSERT INTO products VALUES (3, 'Keyboard', 'Electronics', 79, 23, true)",
    ];
    
    for insert in inserts {
        db.execute(insert)?;
    }
    
    // Query data
    let result = db.execute("SELECT * FROM products WHERE in_stock = true")?;
    println!("In-stock products: {:?}", result.rows);
    
    Ok(())
}
```

### Method 3: Using Web Interface

1. Start the server: `cargo run --bin vtdb`
2. Open `http://localhost:8080` in your browser
3. **Table Explorer**: Use the sidebar on the left to view all tables in your database
   - Click the refresh button (↻) to reload the table list
   - Click on any table name to automatically insert `SELECT * FROM <table>` into the editor
4. Execute SQL queries directly in the Monaco editor
5. Use the example query buttons or type your own queries

---

## Tips for LLM Dataset Generation

When instructing an LLM to generate sample datasets for VTDB:

1. **Specify the table schema** with column names and data types
2. **Request SQL INSERT statements** in the format: `INSERT INTO table_name VALUES (value1, value2, ...)`
3. **Ensure data type compatibility:**
   - INTEGER values should be numbers without quotes
   - VARCHAR values should be strings in single quotes: `'text'`
   - BOOLEAN values should be `true` or `false` (lowercase)
4. **Provide realistic sample data** appropriate for the domain
5. **Include multiple rows** (10-100 rows is a good starting point)
6. **Consider relationships** if generating multiple related tables

**Example LLM Prompt:**
```
Generate a sample dataset for VTDB with the following schema:
- Table: products (id INTEGER, name VARCHAR, category VARCHAR, price INTEGER, stock_quantity INTEGER, in_stock BOOLEAN)
- Generate 20 rows of realistic e-commerce product data
- Output SQL INSERT statements that can be executed directly
- Ensure all data types match the schema
```

---

## Performance Optimizations

VTDB includes several performance optimizations for efficient data operations:

### Batch Insert Optimization

VTDB uses optimized batch insertion for improved performance when inserting data:

- **Columnar Batch Insertion**: Multiple rows are inserted using columnar batch operations, which is significantly faster than row-by-row insertion
- **Memory Pre-allocation**: Column vectors are pre-allocated with the expected capacity to avoid reallocations
- **Single Validation**: Batch validation checks all rows once before insertion, reducing overhead
- **Batched WAL Writes**: Write-Ahead Log entries are written once per batch instead of per-row

**Performance Benefits:**
- **3-5x faster** for batches of 100+ rows compared to row-by-row insertion
- **Reduced memory allocations** through pre-allocation and bulk operations
- **Lower overhead** for large data loading operations

**Best Practices:**
- When loading large datasets, insert multiple rows in sequence - each INSERT statement benefits from batch optimization
- For bulk loading, consider inserting rows in batches of 100-1000 rows per transaction for optimal performance
- The optimization is automatic - no special syntax or configuration needed

### Table Explorer

The web interface includes a table explorer sidebar that provides:
- **Quick table discovery**: View all tables in your database at a glance
- **Fast query generation**: Click any table to generate a `SELECT * FROM <table>` query
- **Real-time updates**: Refresh button to reload the table list after creating new tables

---

## Error Handling

When using the REST API, always check the `success` field in the response:

```python
response = execute_query("SELECT * FROM nonexistent_table")
if not response["success"]:
    print(f"Error: {response['error']}")
else:
    print(f"Results: {response['result']}")
```

Common errors:
- `Table 'table_name' does not exist` - Table hasn't been created yet
- `Column 'column_name' not found` - Column name is incorrect
- `Unsupported data type` - Data type not supported by VTDB
- `Invalid SQL syntax` - SQL statement has syntax errors

---

## Limitations

- **No transactions**: Each query is executed independently
- **In-memory storage**: Data is lost when the server restarts (unless persistence is enabled)
- **Single connection**: The REST API uses a single shared database instance
- **Basic SQL**: Only basic CRUD operations are supported (no JOINs, subqueries, etc.)
- **Limited data types**: Only INTEGER, VARCHAR, and BOOLEAN are supported

---

## Next Steps

- Check the [README.md](README.md) for project overview
- Explore the web interface at `http://localhost:8080`
- Review test files in `tests/` for more examples
- Check the source code for implementation details

