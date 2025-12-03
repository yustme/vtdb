use anyhow::{anyhow, Result};
use crate::catalog::Catalog;
use crate::planner::physical::PhysicalPlan;
use crate::storage::StorageEngine;
use crate::Value;
use crate::QueryResult;

/// Query executor that executes physical plans
#[derive(Clone)]
pub struct Executor {
    // Executor state
}

impl Executor {
    pub fn new() -> Self {
        Self {}
    }

    /// Execute a physical plan
    pub fn execute(
        &self,
        plan: &PhysicalPlan,
        storage: &mut StorageEngine,
        catalog: &mut Catalog,
    ) -> Result<QueryResult> {
        match plan {
            PhysicalPlan::CreateTable { name, columns } => {
                self.execute_create_table(name, columns, storage, catalog)
            }
            PhysicalPlan::Select { table, columns, filter, group_by, limit } => {
                self.execute_select(table, columns, filter, group_by, limit, storage, catalog)
            }
            PhysicalPlan::Insert { table, columns, values } => {
                self.execute_insert(table, columns, values, storage, catalog)
            }
            PhysicalPlan::Update { table, assignments, filter } => {
                self.execute_update(table, assignments, filter, storage, catalog)
            }
            PhysicalPlan::Delete { table, filter } => {
                self.execute_delete(table, filter, storage, catalog)
            }
        }
    }

    fn execute_create_table(
        &self,
        name: &str,
        columns: &[(String, crate::catalog::types::DataType)],
        storage: &mut StorageEngine,
        catalog: &mut Catalog,
    ) -> Result<QueryResult> {
        // Create in catalog
        let column_defs: Vec<(String, crate::catalog::types::DataType)> = columns.to_vec();
        catalog.create_table(name.to_string(), column_defs.clone())?;

        // Create in storage
        storage.create_table(name.to_string(), columns.len())?;

        Ok(QueryResult {
            rows: Vec::new(),
            columns: Vec::new(),
        })
    }

    fn execute_select(
        &self,
        table: &str,
        columns: &[crate::parser::ast::SelectItem],
        filter: &Option<crate::parser::ast::Expr>,
        group_by: &Option<Vec<String>>,
        limit: &Option<u64>,
        storage: &StorageEngine,
        catalog: &Catalog,
    ) -> Result<QueryResult> {
        let table_schema = catalog.get_table(table)?;
        
        // Scan table
        let all_rows = storage.scan_table(table)?;

        // Apply filter
        let filtered_rows = if let Some(filter_expr) = filter {
            all_rows
                .into_iter()
                .filter(|row| {
                    evaluate_predicate(filter_expr, row, table_schema).unwrap_or(false)
                })
                .collect()
        } else {
            all_rows
        };

        // Check if we have aggregate functions
        let has_aggregates = columns.iter().any(|item| {
            matches!(item, crate::parser::ast::SelectItem::FunctionCall { .. })
        });

        // If we have aggregates, perform aggregation
        if has_aggregates {
            return self.execute_aggregation(
                table_schema,
                &filtered_rows,
                columns,
                group_by,
                limit,
            );
        }

        // Apply LIMIT (Snowflake LIMIT behavior: limit the number of rows returned)
        let limited_rows = if let Some(limit_value) = limit {
            let limit_usize = *limit_value as usize;
            let filtered_count = filtered_rows.len();
            let result: Vec<Vec<Value>> = filtered_rows.into_iter().take(limit_usize).collect();
            // Debug: verify LIMIT is being applied
            if result.len() > limit_usize {
                eprintln!("ERROR: LIMIT {} applied but got {} rows!", limit_usize, result.len());
            }
            result
        } else {
            filtered_rows
        };

        // Project columns
        let result_rows = if columns.is_empty() || matches!(columns[0], crate::parser::ast::SelectItem::All) {
            // SELECT *
            limited_rows
        } else {
            // SELECT specific columns
            let mut column_indices = Vec::new();
            let mut result_columns = Vec::new();
            
            for col_item in columns {
                match col_item {
                    crate::parser::ast::SelectItem::Column(col_name) => {
                        let col = table_schema.get_column(col_name).ok_or_else(|| {
                            anyhow!("Column '{}' not found", col_name)
                        })?;
                        column_indices.push(col.ordinal);
                        result_columns.push(col_name.clone());
                    }
                    crate::parser::ast::SelectItem::All => {
                        // Shouldn't happen if we're in this branch
                        return Err(anyhow!("Unexpected SELECT *"));
                    }
                    crate::parser::ast::SelectItem::FunctionCall { .. } => {
                        return Err(anyhow!("Function calls should be handled by aggregation"));
                    }
                }
            }

            limited_rows
                .into_iter()
                .map(|row| {
                    column_indices.iter().map(|&idx| row[idx].clone()).collect()
                })
                .collect()
        };

        // Get column names for result
        let result_columns = if columns.is_empty() || matches!(columns[0], crate::parser::ast::SelectItem::All) {
            table_schema.columns.iter().map(|c| c.name.clone()).collect()
        } else {
            columns.iter()
                .filter_map(|item| {
                    if let crate::parser::ast::SelectItem::Column(name) = item {
                        Some(name.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };

        Ok(QueryResult {
            rows: result_rows,
            columns: result_columns,
        })
    }

    fn execute_aggregation(
        &self,
        table_schema: &crate::catalog::schema::Table,
        filtered_rows: &[Vec<Value>],
        columns: &[crate::parser::ast::SelectItem],
        group_by: &Option<Vec<String>>,
        limit: &Option<u64>,
    ) -> Result<QueryResult> {
        use std::collections::HashMap;

        // Group rows by GROUP BY columns
        let mut groups: HashMap<Vec<Value>, Vec<&Vec<Value>>> = HashMap::new();

        if let Some(group_by_cols) = group_by {
            // Get column indices for GROUP BY columns
            let mut group_by_indices = Vec::new();
            for col_name in group_by_cols {
                let col = table_schema.get_column(col_name).ok_or_else(|| {
                    anyhow!("GROUP BY column '{}' not found", col_name)
                })?;
                group_by_indices.push(col.ordinal);
            }

            // Group rows
            for row in filtered_rows {
                let group_key: Vec<Value> = group_by_indices.iter().map(|&idx| row[idx].clone()).collect();
                groups.entry(group_key).or_insert_with(Vec::new).push(row);
            }
        } else {
            // No GROUP BY - single group with all rows (even if empty)
            // For empty tables, we still need to return one row with aggregate results
            let group_rows: Vec<&Vec<Value>> = filtered_rows.iter().collect();
            groups.insert(Vec::new(), group_rows);
        }
        
        // Ensure we have at least one group if no GROUP BY (for empty tables)
        // This handles the case where filtered_rows is empty
        if group_by.is_none() {
            groups.entry(Vec::new()).or_insert_with(Vec::new);
        }

        // Build column name list
        let mut result_columns = Vec::new();
        for item in columns {
            match item {
                crate::parser::ast::SelectItem::Column(col_name) => {
                    result_columns.push(col_name.clone());
                }
                crate::parser::ast::SelectItem::FunctionCall { name, .. } => {
                    result_columns.push(name.clone());
                }
                crate::parser::ast::SelectItem::All => {
                    return Err(anyhow!("SELECT * not allowed with aggregate functions"));
                }
            }
        }

        // Get GROUP BY column indices if present
        let group_by_indices = if let Some(group_by_cols) = group_by {
            Some(
                group_by_cols
                    .iter()
                    .map(|col_name| {
                        let col = table_schema.get_column(col_name).ok_or_else(|| {
                            anyhow!("GROUP BY column '{}' not found", col_name)
                        })?;
                        Ok(col.ordinal)
                    })
                    .collect::<Result<Vec<_>>>()?,
            )
        } else {
            None
        };

        // Compute aggregates for each group
        let mut result_rows = Vec::new();

        for (group_key, group_rows) in &groups {
            let mut result_row = Vec::new();

            for item in columns {
                match item {
                    crate::parser::ast::SelectItem::Column(col_name) => {
                        // GROUP BY column - get value from group_key
                        if let Some(ref group_by_cols) = group_by {
                            if let Some(idx) = group_by_cols.iter().position(|c| c == col_name) {
                                result_row.push(group_key[idx].clone());
                            } else {
                                return Err(anyhow!("Column '{}' must be in GROUP BY", col_name));
                            }
                        } else {
                            return Err(anyhow!("Column '{}' must be in GROUP BY", col_name));
                        }
                    }
                    crate::parser::ast::SelectItem::FunctionCall { name: _, function } => {
                        let agg_value = compute_aggregate(function, group_rows, table_schema)?;
                        result_row.push(agg_value);
                    }
                    crate::parser::ast::SelectItem::All => {
                        return Err(anyhow!("SELECT * not allowed with aggregate functions"));
                    }
                }
            }

            result_rows.push(result_row);
        }

        // Apply LIMIT after aggregation
        let limited_rows = if let Some(limit_value) = limit {
            let limit_usize = *limit_value as usize;
            result_rows.into_iter().take(limit_usize).collect()
        } else {
            result_rows
        };

        Ok(QueryResult {
            rows: limited_rows,
            columns: result_columns,
        })
    }

    fn execute_insert(
        &self,
        table: &str,
        _columns: &[String],
        values: &[Vec<crate::parser::ast::Expr>],
        storage: &mut StorageEngine,
        catalog: &Catalog,
    ) -> Result<QueryResult> {
        let table_schema = catalog.get_table(table)?;
        
        // Evaluate expressions to get values
        let mut rows = Vec::new();
        for value_exprs in values {
            let mut row = Vec::new();
            for expr in value_exprs {
                let value = evaluate_expr(expr, &[], table_schema)?;
                row.push(value);
            }
            rows.push(row);
        }

        storage.insert_rows(table, rows)?;

        Ok(QueryResult {
            rows: Vec::new(),
            columns: Vec::new(),
        })
    }

    fn execute_update(
        &self,
        table: &str,
        assignments: &[(usize, crate::parser::ast::Expr)],
        filter: &Option<crate::parser::ast::Expr>,
        storage: &mut StorageEngine,
        catalog: &Catalog,
    ) -> Result<QueryResult> {
        let table_schema = catalog.get_table(table)?;
        
        // For each assignment, update matching rows
        for (column_idx, value_expr) in assignments {
            let new_value = evaluate_expr(value_expr, &[], table_schema)?;
            
            let predicate = |row: &[Value]| -> bool {
                if let Some(filter_expr) = filter {
                    evaluate_predicate(filter_expr, row, table_schema).unwrap_or(false)
                } else {
                    true // Update all rows if no filter
                }
            };

            storage.update_rows(table, *column_idx, new_value, predicate)?;
        }

        Ok(QueryResult {
            rows: Vec::new(),
            columns: Vec::new(),
        })
    }

    fn execute_delete(
        &self,
        table: &str,
        filter: &Option<crate::parser::ast::Expr>,
        storage: &mut StorageEngine,
        catalog: &Catalog,
    ) -> Result<QueryResult> {
        let table_schema = catalog.get_table(table)?;
        
        let predicate = |row: &[Value]| -> bool {
            if let Some(filter_expr) = filter {
                evaluate_predicate(filter_expr, row, table_schema).unwrap_or(false)
            } else {
                true // Delete all rows if no filter
            }
        };

        storage.delete_rows(table, predicate)?;

        Ok(QueryResult {
            rows: Vec::new(),
            columns: Vec::new(),
        })
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

/// Evaluate an expression to a value
fn evaluate_expr(
    expr: &crate::parser::ast::Expr,
    row: &[Value],
    table_schema: &crate::catalog::schema::Table,
) -> Result<Value> {
    match expr {
        crate::parser::ast::Expr::Column(col_name) => {
            let col = table_schema.get_column(col_name).ok_or_else(|| {
                anyhow!("Column '{}' not found", col_name)
            })?;
            Ok(row[col.ordinal].clone())
        }
        crate::parser::ast::Expr::Literal(val) => Ok(val.clone()),
        crate::parser::ast::Expr::BinaryOp { left, op, right } => {
            let left_val = evaluate_expr(left, row, table_schema)?;
            let right_val = evaluate_expr(right, row, table_schema)?;
            evaluate_binary_op(&left_val, op, &right_val)
        }
    }
}

/// Evaluate a predicate expression
fn evaluate_predicate(
    expr: &crate::parser::ast::Expr,
    row: &[Value],
    table_schema: &crate::catalog::schema::Table,
) -> Result<bool> {
    match expr {
        crate::parser::ast::Expr::BinaryOp { left, op, right } => {
            match op {
                crate::parser::ast::BinaryOperator::And => {
                    Ok(evaluate_predicate(left, row, table_schema)? && evaluate_predicate(right, row, table_schema)?)
                }
                crate::parser::ast::BinaryOperator::Or => {
                    Ok(evaluate_predicate(left, row, table_schema)? || evaluate_predicate(right, row, table_schema)?)
                }
                _ => {
                    // For comparison operators, evaluate both sides and compare
                    let left_val = evaluate_expr(left, row, table_schema)?;
                    let right_val = evaluate_expr(right, row, table_schema)?;
                    
                    match op {
                        crate::parser::ast::BinaryOperator::Eq => Ok(left_val == right_val),
                        crate::parser::ast::BinaryOperator::Ne => Ok(left_val != right_val),
                        crate::parser::ast::BinaryOperator::Lt => {
                            compare_values(&left_val, &right_val).map(|ord| ord == std::cmp::Ordering::Less)
                        }
                        crate::parser::ast::BinaryOperator::Gt => {
                            compare_values(&left_val, &right_val).map(|ord| ord == std::cmp::Ordering::Greater)
                        }
                        crate::parser::ast::BinaryOperator::Le => {
                            compare_values(&left_val, &right_val).map(|ord| ord != std::cmp::Ordering::Greater)
                        }
                        crate::parser::ast::BinaryOperator::Ge => {
                            compare_values(&left_val, &right_val).map(|ord| ord != std::cmp::Ordering::Less)
                        }
                        _ => Err(anyhow!("Unsupported operator in predicate")),
                    }
                }
            }
        }
        _ => Err(anyhow!("Invalid predicate expression")),
    }
}

/// Compare two values
fn compare_values(left: &Value, right: &Value) -> Result<std::cmp::Ordering> {
    match (left, right) {
        (Value::Integer(l), Value::Integer(r)) => Ok(l.cmp(r)),
        (Value::Varchar(l), Value::Varchar(r)) => Ok(l.cmp(r)),
        (Value::Boolean(l), Value::Boolean(r)) => Ok(l.cmp(r)),
        _ => Err(anyhow!("Cannot compare different types")),
    }
}

/// Evaluate a binary operation
fn evaluate_binary_op(
    left: &Value,
    op: &crate::parser::ast::BinaryOperator,
    right: &Value,
) -> Result<Value> {
    // For now, binary ops in expressions are mainly for comparisons in WHERE clauses
    // Arithmetic operations would go here if needed
    Err(anyhow!("Binary operations in expressions not yet supported"))
}

/// Compute aggregate function value for a group of rows
fn compute_aggregate(
    function: &crate::parser::ast::AggregateFunction,
    rows: &[&Vec<Value>],
    table_schema: &crate::catalog::schema::Table,
) -> Result<Value> {
    use std::collections::HashSet;

    match function {
        crate::parser::ast::AggregateFunction::Count { distinct, expr } => {
            if expr.is_none() {
                // COUNT(*) - count all rows
                Ok(Value::Integer(rows.len() as i64))
            } else {
                // COUNT(column) or COUNT(DISTINCT column)
                let expr = expr.as_ref().unwrap();
                let mut values = Vec::new();

                for row in rows {
                    match evaluate_expr(expr, row, table_schema) {
                        Ok(val) if val != Value::Null => {
                            values.push(val);
                        }
                        _ => {
                            // Skip NULL values
                        }
                    }
                }

                if *distinct {
                    // COUNT(DISTINCT column)
                    let distinct_count = values.iter().collect::<HashSet<_>>().len();
                    Ok(Value::Integer(distinct_count as i64))
                } else {
                    // COUNT(column)
                    Ok(Value::Integer(values.len() as i64))
                }
            }
        }
        crate::parser::ast::AggregateFunction::Sum { distinct, expr } => {
            let mut values = Vec::new();

            for row in rows {
                match evaluate_expr(expr, row, table_schema) {
                    Ok(val) if val != Value::Null => {
                        values.push(val);
                    }
                    _ => {
                        // Skip NULL values
                    }
                }
            }

            if values.is_empty() {
                return Ok(Value::Null);
            }

            if *distinct {
                // SUM(DISTINCT column)
                let distinct_values: HashSet<Value> = values.iter().cloned().collect();
                let sum: Result<i64> = distinct_values
                    .iter()
                    .map(|v| {
                        match v {
                            Value::Integer(i) => Ok(*i),
                            _ => Err(anyhow!("SUM only works on numeric types")),
                        }
                    })
                    .sum();
                Ok(Value::Integer(sum?))
            } else {
                // SUM(column)
                let sum: Result<i64> = values
                    .iter()
                    .map(|v| {
                        match v {
                            Value::Integer(i) => Ok(*i),
                            _ => Err(anyhow!("SUM only works on numeric types")),
                        }
                    })
                    .sum();
                Ok(Value::Integer(sum?))
            }
        }
        crate::parser::ast::AggregateFunction::Avg { distinct, expr } => {
            let mut values = Vec::new();

            for row in rows {
                match evaluate_expr(expr, row, table_schema) {
                    Ok(val) if val != Value::Null => {
                        values.push(val);
                    }
                    _ => {
                        // Skip NULL values
                    }
                }
            }

            if values.is_empty() {
                return Ok(Value::Null);
            }

            if *distinct {
                // AVG(DISTINCT column)
                let distinct_values: HashSet<Value> = values.iter().cloned().collect();
                let sum: Result<i64> = distinct_values
                    .iter()
                    .map(|v| {
                        match v {
                            Value::Integer(i) => Ok(*i),
                            _ => Err(anyhow!("AVG only works on numeric types")),
                        }
                    })
                    .sum();
                let count = distinct_values.len() as i64;
                Ok(Value::Integer(sum? / count))
            } else {
                // AVG(column)
                let sum: Result<i64> = values
                    .iter()
                    .map(|v| {
                        match v {
                            Value::Integer(i) => Ok(*i),
                            _ => Err(anyhow!("AVG only works on numeric types")),
                        }
                    })
                    .sum();
                let count = values.len() as i64;
                Ok(Value::Integer(sum? / count))
            }
        }
        crate::parser::ast::AggregateFunction::Min { expr } => {
            let mut min_val: Option<Value> = None;

            for row in rows {
                match evaluate_expr(expr, row, table_schema) {
                    Ok(val) if val != Value::Null => {
                        if let Some(ref current_min) = min_val {
                            if compare_values(&val, current_min)? == std::cmp::Ordering::Less {
                                min_val = Some(val);
                            }
                        } else {
                            min_val = Some(val);
                        }
                    }
                    _ => {
                        // Skip NULL values
                    }
                }
            }

            // Return NULL if all values were NULL (Snowflake behavior)
            Ok(min_val.unwrap_or(Value::Null))
        }
        crate::parser::ast::AggregateFunction::Max { expr } => {
            let mut max_val: Option<Value> = None;

            for row in rows {
                match evaluate_expr(expr, row, table_schema) {
                    Ok(val) if val != Value::Null => {
                        if let Some(ref current_max) = max_val {
                            if compare_values(&val, current_max)? == std::cmp::Ordering::Greater {
                                max_val = Some(val);
                            }
                        } else {
                            max_val = Some(val);
                        }
                    }
                    _ => {
                        // Skip NULL values
                    }
                }
            }

            // Return NULL if all values were NULL (Snowflake behavior)
            Ok(max_val.unwrap_or(Value::Null))
        }
    }
}

