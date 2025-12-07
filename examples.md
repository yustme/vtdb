# VTDB SQL Examples

This document contains examples of all SQL statements supported by VTDB.

## Table of Contents

1. [CREATE TABLE](#create-table)
2. [INSERT](#insert)
3. [SELECT](#select)
   - [Basic SELECT](#basic-select)
   - [WHERE Clause](#where-clause)
   - [LIMIT](#limit)
   - [GROUP BY](#group-by)
   - [Aggregate Functions](#aggregate-functions)
   - [JOINs](#joins)
4. [UPDATE](#update)
5. [DELETE](#delete)
6. [DROP ALL TABLES](#drop-all-tables)

---

## CREATE TABLE

Creates a new table with specified columns and data types.

**Supported Data Types:**
- `INTEGER` - Integer numbers
- `VARCHAR` - Variable-length strings
- `BOOLEAN` - Boolean values (true/false)

### Basic Syntax

```sql
CREATE TABLE table_name (
    column1 INTEGER,
    column2 VARCHAR,
    column3 BOOLEAN
)
```

### Examples

```sql
-- Simple table with one column
CREATE TABLE users (id INTEGER)

-- Table with multiple columns
CREATE TABLE products (
    id INTEGER,
    name VARCHAR,
    category VARCHAR,
    price INTEGER,
    stock_quantity INTEGER,
    in_stock BOOLEAN
)

-- Table with all data types
CREATE TABLE employees (
    id INTEGER,
    name VARCHAR,
    age INTEGER,
    active BOOLEAN
)
```

---

## INSERT

Inserts a single row into a table.

**Note:** Each INSERT statement inserts one row. To insert multiple rows, execute multiple INSERT statements.

### Basic Syntax

```sql
INSERT INTO table_name VALUES (value1, value2, value3)
```

### Value Formatting

- **INTEGER**: No quotes, numeric value (e.g., `123`, `-42`)
- **VARCHAR**: Single quotes around string (e.g., `'Hello World'`, `'Alice'`)
- **BOOLEAN**: Lowercase `true` or `false`

**String Escaping:** If a VARCHAR value contains a single quote, escape it by doubling: `'O''Brien'`

### Examples

```sql
-- Insert into a table with INTEGER columns
INSERT INTO users VALUES (1, 'Alice', 25, true)

-- Insert with string containing single quote
INSERT INTO users VALUES (2, 'O''Brien', 30, false)

-- Insert with all data types
INSERT INTO products VALUES (1, 'Laptop', 'Electronics', 999, 45, true)

-- Multiple inserts (execute separately)
INSERT INTO products VALUES (1, 'Laptop', 'Electronics', 999, 45, true)
INSERT INTO products VALUES (2, 'Mouse', 'Electronics', 29, 0, false)
INSERT INTO products VALUES (3, 'Keyboard', 'Electronics', 79, 23, true)
```

---

## SELECT

Retrieves data from one or more tables.

### Basic SELECT

Selects columns from a table.

#### Select All Columns

```sql
SELECT * FROM table_name
```

#### Select Specific Columns

```sql
SELECT column1, column2 FROM table_name
```

#### Examples

```sql
-- Select all columns
SELECT * FROM users

-- Select specific columns
SELECT name, age FROM users

-- Select single column
SELECT name FROM users
```

---

### WHERE Clause

Filters rows based on conditions.

**Supported Operators:**
- `=` - Equals
- `!=` - Not equals
- `<` - Less than
- `>` - Greater than
- `<=` - Less than or equal
- `>=` - Greater than or equal
- `AND` - Logical AND
- `OR` - Logical OR

#### Examples

```sql
-- Simple equality
SELECT * FROM users WHERE id = 1

-- Comparison operators
SELECT * FROM products WHERE price > 100
SELECT * FROM products WHERE price < 50
SELECT * FROM products WHERE price >= 100
SELECT * FROM products WHERE price <= 50

-- Boolean conditions
SELECT * FROM products WHERE in_stock = true
SELECT * FROM products WHERE in_stock = false

-- Multiple conditions with AND
SELECT * FROM products WHERE price > 100 AND in_stock = true
SELECT * FROM users WHERE age > 18 AND active = true

-- Multiple conditions with OR
SELECT * FROM products WHERE price < 50 OR in_stock = false
SELECT * FROM users WHERE age < 18 OR age > 65

-- Complex conditions
SELECT * FROM products 
WHERE (price > 100 AND in_stock = true) 
   OR (price < 50 AND category = 'Electronics')

-- String comparison
SELECT * FROM users WHERE name = 'Alice'
SELECT * FROM products WHERE category = 'Electronics'
```

---

### LIMIT

Limits the number of rows returned.

#### Syntax

```sql
SELECT * FROM table_name LIMIT number
```

#### Examples

```sql
-- Limit to 10 rows
SELECT * FROM users LIMIT 10

-- Limit with WHERE clause
SELECT * FROM products WHERE price > 100 LIMIT 5

-- Limit to 1 row
SELECT * FROM users LIMIT 1

-- Limit with specific columns
SELECT name, age FROM users LIMIT 3
```

---

### GROUP BY

Groups rows by one or more columns, typically used with aggregate functions.

#### Syntax

```sql
SELECT column1, aggregate_function(column2) 
FROM table_name 
GROUP BY column1
```

#### Examples

```sql
-- Group by single column
SELECT category, COUNT(*) FROM products GROUP BY category

-- Group by multiple columns
SELECT category, status, COUNT(*) 
FROM products 
GROUP BY category, status

-- Group by with aggregate functions
SELECT category, COUNT(*), SUM(price) 
FROM products 
GROUP BY category
```

---

### Aggregate Functions

Performs calculations on a set of rows and returns a single value.

**Supported Functions:**
- `COUNT(*)` - Counts all rows
- `COUNT(column)` - Counts non-NULL values in a column
- `COUNT(DISTINCT column)` - Counts distinct non-NULL values
- `SUM(column)` - Sum of values in a column
- `SUM(DISTINCT column)` - Sum of distinct values
- `AVG(column)` - Average of values in a column
- `AVG(DISTINCT column)` - Average of distinct values
- `MIN(column)` - Minimum value in a column
- `MAX(column)` - Maximum value in a column

#### COUNT Examples

```sql
-- Count all rows
SELECT COUNT(*) FROM users

-- Count non-NULL values in a column
SELECT COUNT(name) FROM users

-- Count distinct values
SELECT COUNT(DISTINCT category) FROM products

-- Count with WHERE clause
SELECT COUNT(*) FROM users WHERE age > 18

-- Count with GROUP BY
SELECT category, COUNT(*) FROM products GROUP BY category
SELECT category, COUNT(DISTINCT name) FROM products GROUP BY category
```

#### SUM Examples

```sql
-- Sum all values
SELECT SUM(price) FROM products

-- Sum distinct values
SELECT SUM(DISTINCT price) FROM products

-- Sum with WHERE clause
SELECT SUM(amount) FROM orders WHERE user_id = 1

-- Sum with GROUP BY
SELECT category, SUM(price) FROM products GROUP BY category
```

#### AVG Examples

```sql
-- Average of all values
SELECT AVG(price) FROM products

-- Average of distinct values
SELECT AVG(DISTINCT price) FROM products

-- Average with WHERE clause
SELECT AVG(age) FROM users WHERE active = true

-- Average with GROUP BY
SELECT category, AVG(price) FROM products GROUP BY category
```

#### MIN Examples

```sql
-- Minimum value
SELECT MIN(price) FROM products

-- Minimum with WHERE clause
SELECT MIN(age) FROM users WHERE active = true

-- Minimum with GROUP BY
SELECT category, MIN(price) FROM products GROUP BY category
```

#### MAX Examples

```sql
-- Maximum value
SELECT MAX(price) FROM products

-- Maximum with WHERE clause
SELECT MAX(age) FROM users WHERE active = true

-- Maximum with GROUP BY
SELECT category, MAX(price) FROM products GROUP BY category
```

#### Multiple Aggregate Functions

```sql
-- Multiple aggregates
SELECT 
    COUNT(*) as total,
    SUM(price) as total_price,
    AVG(price) as avg_price,
    MIN(price) as min_price,
    MAX(price) as max_price
FROM products

-- Multiple aggregates with GROUP BY
SELECT 
    category,
    COUNT(*) as count,
    SUM(price) as total,
    AVG(price) as average
FROM products
GROUP BY category
```

---

### JOINs

Combines rows from two or more tables based on a related column.

**Supported JOIN Types:**
- `INNER JOIN` - Returns rows that have matching values in both tables
- `LEFT JOIN` (or `LEFT OUTER JOIN`) - Returns all rows from the left table and matched rows from the right table
- `RIGHT JOIN` (or `RIGHT OUTER JOIN`) - Returns all rows from the right table and matched rows from the left table
- `FULL OUTER JOIN` - Returns all rows when there is a match in either table
- `CROSS JOIN` - Returns the Cartesian product of both tables

#### JOIN Syntax

```sql
SELECT columns
FROM table1
JOIN_TYPE table2 ON table1.column = table2.column
```

#### INNER JOIN Examples

```sql
-- Basic INNER JOIN
SELECT users.name, orders.amount 
FROM users 
INNER JOIN orders ON users.id = orders.user_id

-- INNER JOIN with WHERE clause
SELECT users.name, orders.amount 
FROM users 
INNER JOIN orders ON users.id = orders.user_id 
WHERE orders.amount > 100

-- Multiple INNER JOINs
SELECT users.name, orders.amount, products.name
FROM users
INNER JOIN orders ON users.id = orders.user_id
INNER JOIN products ON orders.product_id = products.id
```

#### LEFT JOIN Examples

```sql
-- LEFT JOIN - includes all users even without orders
SELECT users.name, orders.amount 
FROM users 
LEFT JOIN orders ON users.id = orders.user_id

-- LEFT JOIN with WHERE clause
SELECT users.name, orders.amount 
FROM users 
LEFT JOIN orders ON users.id = orders.user_id 
WHERE orders.amount IS NULL
```

#### RIGHT JOIN Examples

```sql
-- RIGHT JOIN - includes all orders even without users
SELECT users.name, orders.amount 
FROM users 
RIGHT JOIN orders ON users.id = orders.user_id
```

#### FULL OUTER JOIN Examples

```sql
-- FULL OUTER JOIN - includes all rows from both tables
SELECT users.name, orders.amount 
FROM users 
FULL OUTER JOIN orders ON users.id = orders.user_id
```

#### CROSS JOIN Examples

```sql
-- CROSS JOIN - Cartesian product
SELECT * FROM table1 CROSS JOIN table2

-- CROSS JOIN with WHERE clause
SELECT * FROM table1 CROSS JOIN table2 WHERE table1.id = 1
```

#### JOIN with USING Clause

```sql
-- JOIN with USING (when column names match)
SELECT users.name, orders.amount 
FROM users 
INNER JOIN orders USING (id)
```

#### JOIN with Qualified Column Names

```sql
-- Using table.column notation
SELECT users.name, orders.amount, products.name
FROM users
INNER JOIN orders ON users.id = orders.user_id
INNER JOIN products ON orders.product_id = products.id
WHERE users.active = true
```

#### JOIN with Aggregate Functions

```sql
-- Aggregate with JOIN
SELECT users.name, COUNT(orders.id) as order_count
FROM users
LEFT JOIN orders ON users.id = orders.user_id
GROUP BY users.name

-- Aggregate with JOIN and WHERE
SELECT users.name, SUM(orders.amount) as total_amount
FROM users
INNER JOIN orders ON users.id = orders.user_id
WHERE orders.created_date > '2024-01-01'
GROUP BY users.name
```

---

## UPDATE

Modifies existing rows in a table.

### Syntax

```sql
UPDATE table_name 
SET column1 = value1, column2 = value2 
WHERE condition
```

### Examples

```sql
-- Update single column
UPDATE users SET name = 'Alice' WHERE id = 1

-- Update multiple columns
UPDATE products SET price = 899, in_stock = true WHERE id = 1

-- Update with condition
UPDATE products SET in_stock = false WHERE stock_quantity = 0

-- Update all rows (no WHERE clause)
UPDATE products SET in_stock = true

-- Update with complex condition
UPDATE products 
SET price = price - 10 
WHERE category = 'Electronics' AND price > 100

-- Update boolean values
UPDATE users SET active = true WHERE age > 18
UPDATE users SET active = false WHERE age < 18
```

---

## DELETE

Removes rows from a table.

### Syntax

```sql
DELETE FROM table_name WHERE condition
```

**Warning:** If no WHERE clause is specified, all rows will be deleted.

### Examples

```sql
-- Delete specific row
DELETE FROM users WHERE id = 1

-- Delete with condition
DELETE FROM products WHERE price < 10
DELETE FROM users WHERE active = false

-- Delete with complex condition
DELETE FROM products 
WHERE category = 'Electronics' AND stock_quantity = 0

-- Delete all rows (use with caution!)
DELETE FROM users
```

---

## DROP ALL TABLES

Removes all tables from the database.

**Warning:** This operation cannot be undone. All data will be permanently deleted.

### Syntax

```sql
DROP ALL TABLES
```

### Example

```sql
DROP ALL TABLES
```

---

## Complete Example Workflow

Here's a complete example showing how to use multiple SQL statements together:

```sql
-- 1. Create tables
CREATE TABLE users (
    id INTEGER,
    name VARCHAR,
    email VARCHAR,
    age INTEGER,
    active BOOLEAN
)

CREATE TABLE orders (
    id INTEGER,
    user_id INTEGER,
    product_name VARCHAR,
    amount INTEGER
)

-- 2. Insert data
INSERT INTO users VALUES (1, 'Alice', 'alice@example.com', 25, true)
INSERT INTO users VALUES (2, 'Bob', 'bob@example.com', 30, true)
INSERT INTO users VALUES (3, 'Charlie', 'charlie@example.com', 35, false)

INSERT INTO orders VALUES (1, 1, 'Laptop', 999)
INSERT INTO orders VALUES (2, 1, 'Mouse', 29)
INSERT INTO orders VALUES (3, 2, 'Keyboard', 79)

-- 3. Query data
SELECT * FROM users
SELECT name, email FROM users WHERE active = true
SELECT COUNT(*) FROM users WHERE age > 25

-- 4. JOIN queries
SELECT users.name, orders.product_name, orders.amount
FROM users
INNER JOIN orders ON users.id = orders.user_id

SELECT users.name, COUNT(orders.id) as order_count
FROM users
LEFT JOIN orders ON users.id = orders.user_id
GROUP BY users.name

-- 5. Aggregate queries
SELECT 
    COUNT(*) as total_users,
    AVG(age) as avg_age,
    MIN(age) as min_age,
    MAX(age) as max_age
FROM users
WHERE active = true

-- 6. Update data
UPDATE users SET active = false WHERE age > 30
UPDATE orders SET amount = 899 WHERE id = 1

-- 7. Delete data
DELETE FROM orders WHERE amount < 50
DELETE FROM users WHERE active = false AND age > 30

-- 8. Final queries
SELECT * FROM users
SELECT * FROM orders
```

---

## Notes

- **Data Types**: Only `INTEGER`, `VARCHAR`, and `BOOLEAN` are supported
- **String Escaping**: Single quotes in strings must be escaped by doubling: `'O''Brien'`
- **Boolean Values**: Must be lowercase `true` or `false`
- **Case Sensitivity**: Table and column names are case-insensitive (stored in uppercase)
- **NULL Values**: NULL values are supported and are ignored by aggregate functions (except COUNT(*))
- **Transactions**: Each SQL statement executes independently (no transactions)
- **Multiple Inserts**: Each INSERT statement inserts one row; execute multiple statements for multiple rows

