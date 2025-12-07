# VTDB Usage Guide for LLMs

This document provides complete information for LLMs to understand how to use VTDB and generate scripts for data loading, querying, and database operations.

## Quick Reference

**VTDB Server:** HTTP REST API at `http://localhost:8080`  
**Main Endpoint:** `POST /api/execute`  
**Supported Data Types:** INTEGER, VARCHAR, BOOLEAN  
**SQL Dialect:** Snowflake-compatible  

---

## API Specification

### Execute SQL Query

**Endpoint:** `POST http://localhost:8080/api/execute`  
**Content-Type:** `application/json`

**Request Body:**
```json
{
  "query": "SQL statement here"
}
```

**Success Response:**
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

### List Tables

**Endpoint:** `GET http://localhost:8080/api/tables`

**Response:**
```json
{
  "success": true,
  "tables": ["table1", "table2", "table3"],
  "error": null
}
```

---

## Reusable Code Templates

### Python Template for Data Loading

```python
import requests
import json

# Configuration
BASE_URL = "http://localhost:8080/api/execute"

def execute_query(query):
    """
    Execute a SQL query against VTDB.
    
    Args:
        query (str): SQL statement to execute
        
    Returns:
        dict: Response from VTDB API
        
    Raises:
        Exception: If query execution fails
    """
    response = requests.post(
        BASE_URL,
        json={"query": query},
        headers={"Content-Type": "application/json"}
    )
    result = response.json()
    
    if not result.get("success", False):
        error_msg = result.get("error", "Unknown error")
        raise Exception(f"Query failed: {error_msg}")
    
    return result

# Usage pattern:
# 1. Create table
# 2. Insert data (loop through rows)
# 3. Query data (optional)
```

### Complete Python Data Loading Script Template

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
    if not result.get("success", False):
        raise Exception(f"Query failed: {result.get('error')}")
    return result

# Step 1: Create table
table_schema = """
CREATE TABLE table_name (
    id INTEGER,
    column1 VARCHAR,
    column2 INTEGER,
    column3 BOOLEAN
)
"""
execute_query(table_schema)
print("Table created successfully")

# Step 2: Insert data
data_rows = [
    (1, "value1", 100, True),
    (2, "value2", 200, False),
    # ... more rows
]

for row in data_rows:
    id_val, col1, col2, col3 = row
    # Format values correctly:
    # - INTEGER: no quotes
    # - VARCHAR: single quotes around string
    # - BOOLEAN: true or false (lowercase)
    insert_query = f"INSERT INTO table_name VALUES ({id_val}, '{col1}', {col2}, {col3})"
    execute_query(insert_query)
    print(f"Inserted row: {id_val}")

# Step 3: Verify data (optional)
result = execute_query("SELECT * FROM table_name")
print(f"Total rows: {len(result['result']['rows'])}")
```

---

## Data Type Formatting Rules

When generating SQL INSERT statements, format values as follows:

| Data Type | Format | Example | Notes |
|-----------|--------|---------|-------|
| INTEGER | No quotes, numeric | `123`, `-42`, `1000` | Direct number |
| VARCHAR | Single quotes | `'Hello World'`, `'Alice'` | Use single quotes, escape single quotes with `''` |
| BOOLEAN | Lowercase | `true`, `false` | Must be lowercase |

**String Escaping:** If a VARCHAR value contains a single quote, escape it by doubling: `'O''Brien'` becomes `'O''Brien'`

**Example:**
```sql
INSERT INTO users VALUES (1, 'John O''Brien', 30, true)
```

---

## SQL Syntax Reference

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
    category VARCHAR,
    price INTEGER,
    stock_quantity INTEGER,
    in_stock BOOLEAN
)
```

### INSERT

```sql
INSERT INTO table_name VALUES (value1, value2, value3)
```

**Example:**
```sql
INSERT INTO products VALUES (1, 'Laptop', 'Electronics', 999, 45, true)
INSERT INTO products VALUES (2, 'Mouse', 'Electronics', 29, 0, false)
```

**Important:** Each INSERT statement inserts one row. For multiple rows, execute multiple INSERT statements sequentially.

### SELECT

```sql
SELECT column1, column2 FROM table_name WHERE condition
SELECT * FROM table_name WHERE condition
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

**Examples:**
```sql
SELECT * FROM products
SELECT name, price FROM products WHERE price > 100
SELECT * FROM products WHERE name = 'Laptop' AND in_stock = true
SELECT * FROM products WHERE price < 50 OR in_stock = false
```

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

## Common Use Case Patterns

### Pattern 1: Load Data from List/Tuple

```python
import requests

BASE_URL = "http://localhost:8080/api/execute"

def execute_query(query):
    response = requests.post(BASE_URL, json={"query": query}, headers={"Content-Type": "application/json"})
    result = response.json()
    if not result.get("success"):
        raise Exception(result.get("error"))
    return result

# Define data
data = [
    (1, "Alice", "alice@example.com", 25, True),
    (2, "Bob", "bob@example.com", 30, False),
    (3, "Charlie", "charlie@example.com", 35, True),
]

# Create table
execute_query("""
    CREATE TABLE users (
        id INTEGER,
        name VARCHAR,
        email VARCHAR,
        age INTEGER,
        active BOOLEAN
    )
""")

# Insert data
for id_val, name, email, age, active in data:
    query = f"INSERT INTO users VALUES ({id_val}, '{name}', '{email}', {age}, {active})"
    execute_query(query)
```

### Pattern 2: Load Data from Dictionary

```python
import requests

BASE_URL = "http://localhost:8080/api/execute"

def execute_query(query):
    response = requests.post(BASE_URL, json={"query": query}, headers={"Content-Type": "application/json"})
    result = response.json()
    if not result.get("success"):
        raise Exception(result.get("error"))
    return result

# Define data as dictionaries
data = [
    {"id": 1, "name": "Laptop", "price": 999, "in_stock": True},
    {"id": 2, "name": "Mouse", "price": 29, "in_stock": False},
    {"id": 3, "name": "Keyboard", "price": 79, "in_stock": True},
]

# Create table
execute_query("""
    CREATE TABLE products (
        id INTEGER,
        name VARCHAR,
        price INTEGER,
        in_stock BOOLEAN
    )
""")

# Insert data
for row in data:
    name_escaped = row["name"].replace("'", "''")  # Escape single quotes
    query = f"INSERT INTO products VALUES ({row['id']}, '{name_escaped}', {row['price']}, {row['in_stock']})"
    execute_query(query)
```

### Pattern 3: Generate and Load Sample Data

```python
import requests
import random

BASE_URL = "http://localhost:8080/api/execute"

def execute_query(query):
    response = requests.post(BASE_URL, json={"query": query}, headers={"Content-Type": "application/json"})
    result = response.json()
    if not result.get("success"):
        raise Exception(result.get("error"))
    return result

# Create table
execute_query("""
    CREATE TABLE sales (
        id INTEGER,
        product_id INTEGER,
        customer_id INTEGER,
        quantity INTEGER,
        total_amount INTEGER,
        sale_date VARCHAR
    )
""")

# Generate and insert sample data
for i in range(1, 101):  # Generate 100 rows
    product_id = random.randint(1, 10)
    customer_id = random.randint(1, 50)
    quantity = random.randint(1, 10)
    total_amount = random.randint(10, 5000)
    sale_date = f"2024-{random.randint(1,12):02d}-{random.randint(1,28):02d}"
    
    query = f"INSERT INTO sales VALUES ({i}, {product_id}, {customer_id}, {quantity}, {total_amount}, '{sale_date}')"
    execute_query(query)
    
    if i % 10 == 0:
        print(f"Inserted {i} rows...")
```

### Pattern 4: Load Data with Error Handling

```python
import requests
import sys

BASE_URL = "http://localhost:8080/api/execute"

def execute_query(query, verbose=True):
    try:
        response = requests.post(
            BASE_URL,
            json={"query": query},
            headers={"Content-Type": "application/json"},
            timeout=30
        )
        result = response.json()
        
        if not result.get("success"):
            error = result.get("error", "Unknown error")
            if verbose:
                print(f"Error executing query: {error}")
                print(f"Query: {query}")
            return None
        
        return result
    except requests.exceptions.RequestException as e:
        if verbose:
            print(f"Request failed: {e}")
        return None
    except Exception as e:
        if verbose:
            print(f"Unexpected error: {e}")
        return None

# Create table
result = execute_query("""
    CREATE TABLE products (
        id INTEGER,
        name VARCHAR,
        price INTEGER
    )
""")

if result is None:
    print("Failed to create table")
    sys.exit(1)

# Load data with error handling
data = [
    (1, "Laptop", 999),
    (2, "Mouse", 29),
    (3, "Keyboard", 79),
]

success_count = 0
for id_val, name, price in data:
    query = f"INSERT INTO products VALUES ({id_val}, '{name}', {price})"
    result = execute_query(query)
    if result:
        success_count += 1
        print(f"Inserted: {name}")
    else:
        print(f"Failed to insert: {name}")

print(f"\nSuccessfully inserted {success_count} out of {len(data)} rows")
```

---

## Complete Example Scripts

### Example 1: E-Commerce Products

```python
import requests

BASE_URL = "http://localhost:8080/api/execute"

def execute_query(query):
    response = requests.post(BASE_URL, json={"query": query}, headers={"Content-Type": "application/json"})
    result = response.json()
    if not result.get("success"):
        raise Exception(f"Query failed: {result.get('error')}")
    return result

# Create products table
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

# Insert product data
products = [
    (1, "Laptop", "Electronics", 999, 45, True),
    (2, "Mouse", "Electronics", 29, 0, False),
    (3, "Keyboard", "Electronics", 79, 23, True),
    (4, "Monitor", "Electronics", 299, 12, True),
    (5, "Desk Chair", "Furniture", 199, 8, True),
    (6, "Desk", "Furniture", 299, 5, True),
    (7, "Lamp", "Furniture", 49, 15, True),
    (8, "Notebook", "Office Supplies", 5, 100, True),
    (9, "Pen", "Office Supplies", 2, 200, True),
    (10, "Stapler", "Office Supplies", 15, 30, True),
]

for id_val, name, category, price, stock, in_stock in products:
    query = f"INSERT INTO products VALUES ({id_val}, '{name}', '{category}', {price}, {stock}, {in_stock})"
    execute_query(query)
    print(f"Inserted: {name}")

# Verify
result = execute_query("SELECT COUNT(*) FROM products")
print(f"\nTotal products: {result['result']['rows'][0][0]}")
```

### Example 2: User Management

```python
import requests
import random

BASE_URL = "http://localhost:8080/api/execute"

def execute_query(query):
    response = requests.post(BASE_URL, json={"query": query}, headers={"Content-Type": "application/json"})
    result = response.json()
    if not result.get("success"):
        raise Exception(f"Query failed: {result.get('error')}")
    return result

# Create users table
execute_query("""
    CREATE TABLE users (
        id INTEGER,
        username VARCHAR,
        email VARCHAR,
        age INTEGER,
        active BOOLEAN
    )
""")

# Generate and insert user data
names = ["Alice", "Bob", "Charlie", "Diana", "Eve", "Frank", "Grace", "Henry"]
domains = ["example.com", "test.com", "demo.org"]

for i in range(1, 21):
    name = random.choice(names)
    username = name.lower()
    email = f"{username}@{random.choice(domains)}"
    age = random.randint(18, 65)
    active = random.choice([True, False])
    
    query = f"INSERT INTO users VALUES ({i}, '{username}', '{email}', {age}, {active})"
    execute_query(query)

print("Inserted 20 users")

# Query active users
result = execute_query("SELECT * FROM users WHERE active = true")
print(f"\nActive users: {len(result['result']['rows'])}")
```

---

## Error Handling Guide

### Common Errors and Solutions

| Error Message | Cause | Solution |
|---------------|-------|----------|
| `Table 'table_name' does not exist` | Table not created | Create table first with CREATE TABLE |
| `Column 'column_name' not found` | Wrong column name | Check column names match schema |
| `Unsupported data type` | Invalid data type | Use only INTEGER, VARCHAR, BOOLEAN |
| `Invalid SQL syntax` | SQL syntax error | Check SQL statement syntax |
| Connection refused | Server not running | Start VTDB server first |

### Error Handling Pattern

```python
import requests

BASE_URL = "http://localhost:8080/api/execute"

def execute_query(query):
    response = requests.post(BASE_URL, json={"query": query}, headers={"Content-Type": "application/json"})
    result = response.json()
    
    if not result.get("success"):
        error = result.get("error", "Unknown error")
        print(f"Error: {error}")
        print(f"Query: {query}")
        return None
    
    return result

# Use with error checking
result = execute_query("SELECT * FROM nonexistent_table")
if result is None:
    print("Query failed, skipping...")
else:
    print(f"Results: {result['result']['rows']}")
```

---

## Server Setup

### Using Docker (Recommended)

```bash
docker-compose up --build
```

Server will be available at `http://localhost:8080`

### Running Locally

```bash
./start-server.sh
# or
cargo run --release
```

---

## Key Points for LLMs

When generating scripts for VTDB:

1. **Always create the table first** using CREATE TABLE before inserting data
2. **Format values correctly:**
   - INTEGER: no quotes (e.g., `123`)
   - VARCHAR: single quotes (e.g., `'text'`)
   - BOOLEAN: lowercase `true` or `false`
3. **Escape single quotes in strings** by doubling them: `'O''Brien'`
4. **Each INSERT statement inserts one row** - loop through data to insert multiple rows
5. **Check for errors** by examining the `success` field in the response
6. **Use the execute_query pattern** shown in templates for consistency
7. **Server must be running** before executing queries - default URL is `http://localhost:8080`

---

## Limitations

- **No transactions**: Each query executes independently
- **Data types**: Only INTEGER, VARCHAR, BOOLEAN supported
- **Basic SQL**: No JOINs, subqueries, or advanced features
- **Single connection**: REST API uses shared database instance
- **Data persistence**: Data stored in `./data` directory (Iceberg format)

---

## Additional Resources

- See `README.md` for project overview
- Web interface available at `http://localhost:8080` when server is running
- Test files in `tests/` directory provide more examples
