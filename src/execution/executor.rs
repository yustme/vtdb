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

/// Combined schema for JOINs - tracks columns from multiple tables
struct CombinedSchema {
    columns: Vec<ColumnInfo>,
    table_names: Vec<String>,  // Table names/aliases in order
    table_offsets: Vec<usize>,  // Starting column index for each table
}

#[derive(Clone)]
struct ColumnInfo {
    name: String,
    table_name: String,  // Original table name or alias
    ordinal: usize,      // Position in combined row
}

impl CombinedSchema {
    fn from_single_table(
        schema: &crate::catalog::schema::Table,
        alias: Option<String>,
    ) -> Result<Self> {
        let table_name = alias.unwrap_or_else(|| schema.name.clone());
        let mut columns = Vec::new();
        for (idx, col) in schema.columns.iter().enumerate() {
            columns.push(ColumnInfo {
                name: col.name.clone(),
                table_name: table_name.clone(),
                ordinal: idx,
            });
        }
        Ok(CombinedSchema {
            columns,
            table_names: vec![table_name],
            table_offsets: vec![0],
        })
    }

    fn from_join(left: &Self, right: &Self) -> Result<Self> {
        let left_col_count = left.columns.len();
        let mut columns = left.columns.clone();
        
        // Add right columns with adjusted ordinals
        for col in &right.columns {
            columns.push(ColumnInfo {
                name: col.name.clone(),
                table_name: col.table_name.clone(),
                ordinal: col.ordinal + left_col_count,
            });
        }
        
        let mut table_names = left.table_names.clone();
        table_names.extend(right.table_names.clone());
        
        let mut table_offsets = left.table_offsets.clone();
        let right_start = left_col_count;
        for offset in &right.table_offsets {
            table_offsets.push(offset + right_start);
        }
        
        Ok(CombinedSchema {
            columns,
            table_names,
            table_offsets,
        })
    }

    fn find_column(&self, table: Option<&str>, column: &str) -> Result<usize> {
        if let Some(table_name) = table {
            // Qualified column name
            for col in &self.columns {
                if col.table_name == table_name && col.name == column {
                    return Ok(col.ordinal);
                }
            }
            Err(anyhow!("Column '{}.{}' not found", table_name, column))
        } else {
            // Unqualified - find unique match
            let matches: Vec<&ColumnInfo> = self.columns
                .iter()
                .filter(|c| c.name == column)
                .collect();
            
            if matches.is_empty() {
                Err(anyhow!("Column '{}' not found", column))
            } else if matches.len() > 1 {
                Err(anyhow!("Column '{}' is ambiguous (appears in multiple tables)", column))
            } else {
                Ok(matches[0].ordinal)
            }
        }
    }

    fn get_all_column_names(&self) -> Vec<String> {
        self.columns.iter().map(|c| c.name.clone()).collect()
    }
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
            PhysicalPlan::Select { from, columns, filter, group_by, limit } => {
                self.execute_select(from, columns, filter, group_by, limit, storage, catalog)
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
        from: &crate::parser::ast::TableRef,
        columns: &[crate::parser::ast::SelectItem],
        filter: &Option<crate::parser::ast::Expr>,
        group_by: &Option<Vec<String>>,
        limit: &Option<u64>,
        storage: &StorageEngine,
        catalog: &Catalog,
    ) -> Result<QueryResult> {
        // Execute JOINs or single table scan
        let (all_rows, combined_schema) = self.execute_table_ref(from, storage, catalog)?;

        // Apply filter
        let filtered_rows = if let Some(filter_expr) = filter {
            all_rows
                .into_iter()
                .filter(|row| {
                    self.evaluate_predicate_with_schema(filter_expr, row, &combined_schema).unwrap_or(false)
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
            return self.execute_aggregation_with_schema(
                &combined_schema,
                &filtered_rows,
                columns,
                group_by,
                limit,
            );
        }

        // Apply LIMIT (Snowflake LIMIT behavior: limit the number of rows returned)
        let limited_rows = if let Some(limit_value) = limit {
            let limit_usize = *limit_value as usize;
            let result: Vec<Vec<Value>> = filtered_rows.into_iter().take(limit_usize).collect();
            result
        } else {
            filtered_rows
        };

        // Project columns
        let (result_rows, result_columns) = self.project_columns(columns, &limited_rows, &combined_schema)?;

        Ok(QueryResult {
            rows: result_rows,
            columns: result_columns,
        })
    }

    /// Execute a TableRef (table or JOIN) and return rows with combined schema
    fn execute_table_ref(
        &self,
        table_ref: &crate::parser::ast::TableRef,
        storage: &StorageEngine,
        catalog: &Catalog,
    ) -> Result<(Vec<Vec<Value>>, CombinedSchema)> {
        match table_ref {
            crate::parser::ast::TableRef::Table { name, alias } => {
                let schema = catalog.get_table(name)?;
                let rows = storage.scan_table(name)?;
                let combined_schema = CombinedSchema::from_single_table(schema, alias.clone())?;
                Ok((rows, combined_schema))
            }
            crate::parser::ast::TableRef::Join { left, right, join_type, condition } => {
                self.execute_join(left, right, join_type, condition, storage, catalog)
            }
        }
    }

    /// Execute a JOIN operation
    fn execute_join(
        &self,
        left: &crate::parser::ast::TableRef,
        right: &crate::parser::ast::TableRef,
        join_type: &crate::parser::ast::JoinType,
        condition: &Option<crate::parser::ast::JoinCondition>,
        storage: &StorageEngine,
        catalog: &Catalog,
    ) -> Result<(Vec<Vec<Value>>, CombinedSchema)> {
        // Execute left side
        let (left_rows, left_schema) = self.execute_table_ref(left, storage, catalog)?;
        
        // Execute right side
        let (right_rows, right_schema) = self.execute_table_ref(right, storage, catalog)?;
        
        // Combine schemas
        let combined_schema = CombinedSchema::from_join(&left_schema, &right_schema)?;
        
        // Perform JOIN
        let joined_rows = match join_type {
            crate::parser::ast::JoinType::Inner => {
                self.inner_join(&left_rows, &right_rows, condition, &left_schema, &right_schema)?
            }
            crate::parser::ast::JoinType::Left => {
                self.left_join(&left_rows, &right_rows, condition, &left_schema, &right_schema)?
            }
            crate::parser::ast::JoinType::Right => {
                self.right_join(&left_rows, &right_rows, condition, &left_schema, &right_schema)?
            }
            crate::parser::ast::JoinType::FullOuter => {
                self.full_outer_join(&left_rows, &right_rows, condition, &left_schema, &right_schema)?
            }
            crate::parser::ast::JoinType::Cross => {
                self.cross_join(&left_rows, &right_rows)?
            }
        };
        
        Ok((joined_rows, combined_schema))
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
        crate::parser::ast::Expr::QualifiedColumn { table: _, column } => {
            // For single table queries, qualified columns work the same as unqualified
            let col = table_schema.get_column(column).ok_or_else(|| {
                anyhow!("Column '{}' not found", column)
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

    return match function {
        crate::parser::ast::AggregateFunction::Count { distinct, expr } => {
            if expr.is_none() {
                // COUNT(*) - count all rows
                return Ok(Value::Integer(rows.len() as i64));
            }
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
                return Ok(Value::Integer(distinct_count as i64));
            }
            // COUNT(column)
            Ok(Value::Integer(values.len() as i64))
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
                return Ok(Value::Integer(sum?));
            }
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
                return Ok(Value::Integer(sum? / count));
            }
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
    };

impl Executor {
    /// Inner JOIN implementation  
    fn inner_join(
        &self,
        left_rows: &[Vec<Value>],
        right_rows: &[Vec<Value>],
        condition: &Option<crate::parser::ast::JoinCondition>,
        left_schema: &CombinedSchema,
        right_schema: &CombinedSchema,
    ) -> Result<Vec<Vec<Value>>> {
        let mut result = Vec::new();
        
        match condition {
            Some(crate::parser::ast::JoinCondition::On(expr)) => {
                // ON condition - evaluate for each row pair
                for left_row in left_rows {
                    for right_row in right_rows {
                        let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                        if self.evaluate_join_condition(expr, &combined_row, left_schema, right_schema)? {
                            result.push(combined_row);
                        }
                    }
                }
            }
            Some(crate::parser::ast::JoinCondition::Using(columns)) => {
                // USING clause - equi-join on specified columns
                for left_row in left_rows {
                    for right_row in right_rows {
                        let mut matches = true;
                        for col_name in columns {
                            let left_idx = left_schema.find_column(None, col_name)?;
                            let right_idx = right_schema.find_column(None, col_name)?;
                            if left_row[left_idx] != right_row[right_idx] {
                                matches = false;
                                break;
                            }
                        }
                        if matches {
                            let mut combined = left_row.clone();
                            // Add right columns, skipping USING columns
                            let mut right_indices: Vec<usize> = (0..right_row.len()).collect();
                            for col_name in columns {
                                if let Ok(idx) = right_schema.find_column(None, col_name) {
                                    right_indices.retain(|&i| i != idx);
                                }
                            }
                            for &idx in &right_indices {
                                combined.push(right_row[idx].clone());
                            }
                            result.push(combined);
                        }
                    }
                }
            }
            None => {
                // No condition - cartesian product (shouldn't happen for INNER JOIN, but handle it)
                for left_row in left_rows {
                    for right_row in right_rows {
                        let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                        result.push(combined_row);
                    }
                }
            }
        }
        
        Ok(result)
    }

    /// LEFT JOIN implementation
    fn left_join(
        &self,
        left_rows: &[Vec<Value>],
        right_rows: &[Vec<Value>],
        condition: &Option<crate::parser::ast::JoinCondition>,
        left_schema: &CombinedSchema,
        right_schema: &CombinedSchema,
    ) -> Result<Vec<Vec<Value>>> {
        let mut result = Vec::new();
        let right_null_row: Vec<Value> = vec![Value::Null; right_schema.columns.len()];
        
        match condition {
            Some(crate::parser::ast::JoinCondition::On(expr)) => {
                for left_row in left_rows {
                    let mut matched = false;
                    for right_row in right_rows {
                        let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                        if self.evaluate_join_condition(expr, &combined_row, left_schema, right_schema)? {
                            result.push(combined_row);
                            matched = true;
                        }
                    }
                    if !matched {
                        // No match - add left row with NULLs for right
                        let mut combined = left_row.clone();
                        combined.extend(right_null_row.clone());
                        result.push(combined);
                    }
                }
            }
            Some(crate::parser::ast::JoinCondition::Using(columns)) => {
                for left_row in left_rows {
                    let mut matched = false;
                    for right_row in right_rows {
                        let mut matches = true;
                        for col_name in columns {
                            let left_idx = left_schema.find_column(None, col_name)?;
                            let right_idx = right_schema.find_column(None, col_name)?;
                            if left_row[left_idx] != right_row[right_idx] {
                                matches = false;
                                break;
                            }
                        }
                        if matches {
                            matched = true;
                            let mut combined = left_row.clone();
                            let mut right_indices: Vec<usize> = (0..right_row.len()).collect();
                            for col_name in columns {
                                if let Ok(idx) = right_schema.find_column(None, col_name) {
                                    right_indices.retain(|&i| i != idx);
                                }
                            }
                            for &idx in &right_indices {
                                combined.push(right_row[idx].clone());
                            }
                            result.push(combined);
                        }
                    }
                    if !matched {
                        let mut combined = left_row.clone();
                        combined.extend(right_null_row.clone());
                        result.push(combined);
                    }
                }
            }
            None => {
                // CROSS JOIN behavior
                for left_row in left_rows {
                    for right_row in right_rows {
                        let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                        result.push(combined_row);
                    }
                }
            }
        }
        
        Ok(result)
    }

    /// RIGHT JOIN implementation
    fn right_join(
        &self,
        left_rows: &[Vec<Value>],
        right_rows: &[Vec<Value>],
        condition: &Option<crate::parser::ast::JoinCondition>,
        left_schema: &CombinedSchema,
        right_schema: &CombinedSchema,
    ) -> Result<Vec<Vec<Value>>> {
        let mut result = Vec::new();
        let left_null_row: Vec<Value> = vec![Value::Null; left_schema.columns.len()];
        
        match condition {
            Some(crate::parser::ast::JoinCondition::On(expr)) => {
                for right_row in right_rows {
                    let mut matched = false;
                    for left_row in left_rows {
                        let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                        if self.evaluate_join_condition(expr, &combined_row, left_schema, right_schema)? {
                            result.push(combined_row);
                            matched = true;
                        }
                    }
                    if !matched {
                        // No match - add NULLs for left, then right row
                        let mut combined = left_null_row.clone();
                        combined.extend(right_row.clone());
                        result.push(combined);
                    }
                }
            }
            Some(crate::parser::ast::JoinCondition::Using(columns)) => {
                for right_row in right_rows {
                    let mut matched = false;
                    for left_row in left_rows {
                        let mut matches = true;
                        for col_name in columns {
                            let left_idx = left_schema.find_column(None, col_name)?;
                            let right_idx = right_schema.find_column(None, col_name)?;
                            if left_row[left_idx] != right_row[right_idx] {
                                matches = false;
                                break;
                            }
                        }
                        if matches {
                            matched = true;
                            let mut combined = left_row.clone();
                            let mut right_indices: Vec<usize> = (0..right_row.len()).collect();
                            for col_name in columns {
                                if let Ok(idx) = right_schema.find_column(None, col_name) {
                                    right_indices.retain(|&i| i != idx);
                                }
                            }
                            for &idx in &right_indices {
                                combined.push(right_row[idx].clone());
                            }
                            result.push(combined);
                        }
                    }
                    if !matched {
                        let mut combined = left_null_row.clone();
                        combined.extend(right_row.clone());
                        result.push(combined);
                    }
                }
            }
            None => {
                // CROSS JOIN behavior
                for left_row in left_rows {
                    for right_row in right_rows {
                        let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                        result.push(combined_row);
                    }
                }
            }
        }
        
        Ok(result)
    }

    /// FULL OUTER JOIN implementation
    fn full_outer_join(
        &self,
        left_rows: &[Vec<Value>],
        right_rows: &[Vec<Value>],
        condition: &Option<crate::parser::ast::JoinCondition>,
        left_schema: &CombinedSchema,
        right_schema: &CombinedSchema,
    ) -> Result<Vec<Vec<Value>>> {
        use std::collections::HashSet;
        
        let mut result = Vec::new();
        let left_null_row: Vec<Value> = vec![Value::Null; left_schema.columns.len()];
        let right_null_row: Vec<Value> = vec![Value::Null; right_schema.columns.len()];
        let mut matched_left_indices = HashSet::new();
        let mut matched_right_indices = HashSet::new();
        
        match condition {
            Some(crate::parser::ast::JoinCondition::On(expr)) => {
                // Find all matches
                for (left_idx, left_row) in left_rows.iter().enumerate() {
                    for (right_idx, right_row) in right_rows.iter().enumerate() {
                        let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                        if self.evaluate_join_condition(expr, &combined_row, left_schema, right_schema)? {
                            result.push(combined_row);
                            matched_left_indices.insert(left_idx);
                            matched_right_indices.insert(right_idx);
                        }
                    }
                }
                
                // Add unmatched left rows
                for (idx, left_row) in left_rows.iter().enumerate() {
                    if !matched_left_indices.contains(&idx) {
                        let mut combined = left_row.clone();
                        combined.extend(right_null_row.clone());
                        result.push(combined);
                    }
                }
                
                // Add unmatched right rows
                for (idx, right_row) in right_rows.iter().enumerate() {
                    if !matched_right_indices.contains(&idx) {
                        let mut combined = left_null_row.clone();
                        combined.extend(right_row.clone());
                        result.push(combined);
                    }
                }
            }
            Some(crate::parser::ast::JoinCondition::Using(columns)) => {
                // Find all matches
                for (left_idx, left_row) in left_rows.iter().enumerate() {
                    for (right_idx, right_row) in right_rows.iter().enumerate() {
                        let mut matches = true;
                        for col_name in columns {
                            let left_col_idx = left_schema.find_column(None, col_name)?;
                            let right_col_idx = right_schema.find_column(None, col_name)?;
                            if left_row[left_col_idx] != right_row[right_col_idx] {
                                matches = false;
                                break;
                            }
                        }
                        if matches {
                            matched_left_indices.insert(left_idx);
                            matched_right_indices.insert(right_idx);
                            let mut combined = left_row.clone();
                            let mut right_indices: Vec<usize> = (0..right_row.len()).collect();
                            for col_name in columns {
                                if let Ok(idx) = right_schema.find_column(None, col_name) {
                                    right_indices.retain(|&i| i != idx);
                                }
                            }
                            for &idx in &right_indices {
                                combined.push(right_row[idx].clone());
                            }
                            result.push(combined);
                        }
                    }
                }
                
                // Add unmatched left rows
                for (idx, left_row) in left_rows.iter().enumerate() {
                    if !matched_left_indices.contains(&idx) {
                        let mut combined = left_row.clone();
                        combined.extend(right_null_row.clone());
                        result.push(combined);
                    }
                }
                
                // Add unmatched right rows
                for (idx, right_row) in right_rows.iter().enumerate() {
                    if !matched_right_indices.contains(&idx) {
                        let mut combined = left_null_row.clone();
                        combined.extend(right_row.clone());
                        result.push(combined);
                    }
                }
            }
            None => {
                // CROSS JOIN behavior
                for left_row in left_rows {
                    for right_row in right_rows {
                        let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                        result.push(combined_row);
                    }
                }
            }
        }
        
        Ok(result)
    }

    /// CROSS JOIN implementation
    fn cross_join(
        &self,
        left_rows: &[Vec<Value>],
        right_rows: &[Vec<Value>],
    ) -> Result<Vec<Vec<Value>>> {
        let mut result = Vec::new();
        for left_row in left_rows {
            for right_row in right_rows {
                let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                result.push(combined_row);
            }
        }
        Ok(result)
    }

    /// Evaluate JOIN condition expression
    fn evaluate_join_condition(
        &self,
        expr: &crate::parser::ast::Expr,
        combined_row: &[Value],
        left_schema: &CombinedSchema,
        right_schema: &CombinedSchema,
    ) -> Result<bool> {
        match expr {
            crate::parser::ast::Expr::BinaryOp { left, op, right } => {
                match op {
                    crate::parser::ast::BinaryOperator::Eq => {
                        let left_val = self.evaluate_expr_with_schemas(left, combined_row, left_schema, right_schema)?;
                        let right_val = self.evaluate_expr_with_schemas(right, combined_row, left_schema, right_schema)?;
                        Ok(left_val == right_val)
                    }
                    crate::parser::ast::BinaryOperator::Ne => {
                        let left_val = self.evaluate_expr_with_schemas(left, combined_row, left_schema, right_schema)?;
                        let right_val = self.evaluate_expr_with_schemas(right, combined_row, left_schema, right_schema)?;
                        Ok(left_val != right_val)
                    }
                    crate::parser::ast::BinaryOperator::Lt => {
                        let left_val = self.evaluate_expr_with_schemas(left, combined_row, left_schema, right_schema)?;
                        let right_val = self.evaluate_expr_with_schemas(right, combined_row, left_schema, right_schema)?;
                        compare_values(&left_val, &right_val).map(|ord| ord == std::cmp::Ordering::Less)
                    }
                    crate::parser::ast::BinaryOperator::Gt => {
                        let left_val = self.evaluate_expr_with_schemas(left, combined_row, left_schema, right_schema)?;
                        let right_val = self.evaluate_expr_with_schemas(right, combined_row, left_schema, right_schema)?;
                        compare_values(&left_val, &right_val).map(|ord| ord == std::cmp::Ordering::Greater)
                    }
                    crate::parser::ast::BinaryOperator::Le => {
                        let left_val = self.evaluate_expr_with_schemas(left, combined_row, left_schema, right_schema)?;
                        let right_val = self.evaluate_expr_with_schemas(right, combined_row, left_schema, right_schema)?;
                        compare_values(&left_val, &right_val).map(|ord| ord != std::cmp::Ordering::Greater)
                    }
                    crate::parser::ast::BinaryOperator::Ge => {
                        let left_val = self.evaluate_expr_with_schemas(left, combined_row, left_schema, right_schema)?;
                        let right_val = self.evaluate_expr_with_schemas(right, combined_row, left_schema, right_schema)?;
                        compare_values(&left_val, &right_val).map(|ord| ord != std::cmp::Ordering::Less)
                    }
                    crate::parser::ast::BinaryOperator::And => {
                        Ok(self.evaluate_join_condition(left, combined_row, left_schema, right_schema)? &&
                           self.evaluate_join_condition(right, combined_row, left_schema, right_schema)?)
                    }
                    crate::parser::ast::BinaryOperator::Or => {
                        Ok(self.evaluate_join_condition(left, combined_row, left_schema, right_schema)? ||
                           self.evaluate_join_condition(right, combined_row, left_schema, right_schema)?)
                    }
                }
            }
            _ => Err(anyhow!("Unsupported JOIN condition expression")),
        }
    }

    /// Evaluate expression with combined schemas
    fn evaluate_expr_with_schemas(
        &self,
        expr: &crate::parser::ast::Expr,
        combined_row: &[Value],
        left_schema: &CombinedSchema,
        right_schema: &CombinedSchema,
    ) -> Result<Value> {
        match expr {
            crate::parser::ast::Expr::Column(col_name) => {
                // Try left schema first, then right
                if let Ok(idx) = left_schema.find_column(None, col_name) {
                    Ok(combined_row[idx].clone())
                } else if let Ok(idx) = right_schema.find_column(None, col_name) {
                    Ok(combined_row[left_schema.columns.len() + idx].clone())
                } else {
                    Err(anyhow!("Column '{}' not found", col_name))
                }
            }
            crate::parser::ast::Expr::QualifiedColumn { table, column } => {
                // Find in left or right schema
                if let Ok(idx) = left_schema.find_column(Some(table), column) {
                    Ok(combined_row[idx].clone())
                } else if let Ok(idx) = right_schema.find_column(Some(table), column) {
                    Ok(combined_row[left_schema.columns.len() + idx].clone())
                } else {
                    Err(anyhow!("Column '{}.{}' not found", table, column))
                }
            }
            crate::parser::ast::Expr::Literal(val) => Ok(val.clone()),
            crate::parser::ast::Expr::BinaryOp { left, op, right } => {
                let left_val = self.evaluate_expr_with_schemas(left, combined_row, left_schema, right_schema)?;
                let right_val = self.evaluate_expr_with_schemas(right, combined_row, left_schema, right_schema)?;
                evaluate_binary_op(&left_val, op, &right_val)
            }
        }
    }

    /// Project columns from joined rows
    fn project_columns(
        &self,
        columns: &[crate::parser::ast::SelectItem],
        rows: &[Vec<Value>],
        schema: &CombinedSchema,
    ) -> Result<(Vec<Vec<Value>>, Vec<String>)> {
        if columns.is_empty() || matches!(columns[0], crate::parser::ast::SelectItem::All) {
            // SELECT *
            let result_columns = schema.get_all_column_names();
            Ok((rows.to_vec(), result_columns))
        } else {
            let mut column_indices = Vec::new();
            let mut result_columns = Vec::new();
            
            for col_item in columns {
                match col_item {
                    crate::parser::ast::SelectItem::Column(col_name) => {
                        // Handle qualified column names (table.column)
                        let idx = if col_name.contains('.') {
                            let parts: Vec<&str> = col_name.split('.').collect();
                            if parts.len() == 2 {
                                schema.find_column(Some(parts[0]), parts[1])?
                            } else {
                                return Err(anyhow!("Invalid qualified column name: {}", col_name));
                            }
                        } else {
                            schema.find_column(None, col_name)?
                        };
                        column_indices.push(idx);
                        result_columns.push(col_name.clone());
                    }
                    crate::parser::ast::SelectItem::All => {
                        return Err(anyhow!("Unexpected SELECT *"));
                    }
                    crate::parser::ast::SelectItem::FunctionCall { .. } => {
                        return Err(anyhow!("Function calls should be handled by aggregation"));
                    }
                }
            }

            let result_rows: Vec<Vec<Value>> = rows
                .iter()
                .map(|row| {
                    column_indices.iter().map(|&idx| row[idx].clone()).collect()
                })
                .collect();

            Ok((result_rows, result_columns))
        }
    }

    /// Evaluate predicate with combined schema
    fn evaluate_predicate_with_schema(
        &self,
        expr: &crate::parser::ast::Expr,
        row: &[Value],
        schema: &CombinedSchema,
    ) -> Result<bool> {
        match expr {
            crate::parser::ast::Expr::BinaryOp { left, op, right } => {
                match op {
                    crate::parser::ast::BinaryOperator::And => {
                        Ok(self.evaluate_predicate_with_schema(left, row, schema)? &&
                           self.evaluate_predicate_with_schema(right, row, schema)?)
                    }
                    crate::parser::ast::BinaryOperator::Or => {
                        Ok(self.evaluate_predicate_with_schema(left, row, schema)? ||
                           self.evaluate_predicate_with_schema(right, row, schema)?)
                    }
                    _ => {
                        let left_val = self.evaluate_expr_with_combined_schema(left, row, schema)?;
                        let right_val = self.evaluate_expr_with_combined_schema(right, row, schema)?;
                        
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

    /// Evaluate expression with combined schema
    fn evaluate_expr_with_combined_schema(
        &self,
        expr: &crate::parser::ast::Expr,
        row: &[Value],
        schema: &CombinedSchema,
    ) -> Result<Value> {
        match expr {
            crate::parser::ast::Expr::Column(col_name) => {
                let idx = schema.find_column(None, col_name)?;
                Ok(row[idx].clone())
            }
            crate::parser::ast::Expr::QualifiedColumn { table, column } => {
                let idx = schema.find_column(Some(table), column)?;
                Ok(row[idx].clone())
            }
            crate::parser::ast::Expr::Literal(val) => Ok(val.clone()),
            crate::parser::ast::Expr::BinaryOp { left, op, right } => {
                let left_val = self.evaluate_expr_with_combined_schema(left, row, schema)?;
                let right_val = self.evaluate_expr_with_combined_schema(right, row, schema)?;
                evaluate_binary_op(&left_val, op, &right_val)
            }
        }
    }

    /// Execute aggregation with combined schema
    fn execute_aggregation_with_schema(
        &self,
        schema: &CombinedSchema,
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
                let idx = schema.find_column(None, col_name)?;
                group_by_indices.push(idx);
            }

            // Group rows
            for row in filtered_rows {
                let group_key: Vec<Value> = group_by_indices.iter().map(|&idx| row[idx].clone()).collect();
                groups.entry(group_key).or_insert_with(Vec::new).push(row);
            }
        } else {
            // No GROUP BY - single group with all rows (even if empty)
            groups.insert(Vec::new(), filtered_rows.iter().collect());
        }
        
        // Ensure we have at least one group if no GROUP BY (for empty tables)
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
                        let agg_value = self.compute_aggregate_with_schema(function, group_rows, schema)?;
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

    /// Compute aggregate with combined schema
    fn compute_aggregate_with_schema(
        &self,
        function: &crate::parser::ast::AggregateFunction,
        rows: &[&Vec<Value>],
        schema: &CombinedSchema,
    ) -> Result<Value> {
        use std::collections::HashSet;

        match function {
            crate::parser::ast::AggregateFunction::Count { distinct, expr } => {
                if expr.is_none() {
                    Ok(Value::Integer(rows.len() as i64))
                } else {
                    let expr = expr.as_ref().unwrap();
                    let mut values = Vec::new();

                    for row in rows {
                        match self.evaluate_expr_with_combined_schema(expr, row, schema) {
                            Ok(val) if val != Value::Null => {
                                values.push(val);
                            }
                            _ => {}
                        }
                    }

                    if *distinct {
                        let distinct_count = values.iter().collect::<HashSet<_>>().len();
                        Ok(Value::Integer(distinct_count as i64))
                    } else {
                        Ok(Value::Integer(values.len() as i64))
                    }
                }
            }
            crate::parser::ast::AggregateFunction::Sum { distinct, expr } => {
                let mut values = Vec::new();
                for row in rows {
                    match self.evaluate_expr_with_combined_schema(expr, row, schema) {
                        Ok(val) if val != Value::Null => {
                            values.push(val);
                        }
                        _ => {}
                    }
                }
                if values.is_empty() {
                    return Ok(Value::Null);
                }
                if *distinct {
                    let distinct_values: HashSet<Value> = values.iter().cloned().collect();
                    let sum: Result<i64> = distinct_values.iter().map(|v| {
                        match v {
                            Value::Integer(i) => Ok(*i),
                            _ => Err(anyhow!("SUM only works on numeric types")),
                        }
                    }).sum();
                    Ok(Value::Integer(sum?))
                } else {
                    let sum: Result<i64> = values.iter().map(|v| {
                        match v {
                            Value::Integer(i) => Ok(*i),
                            _ => Err(anyhow!("SUM only works on numeric types")),
                        }
                    }).sum();
                    Ok(Value::Integer(sum?))
                }
            }
            crate::parser::ast::AggregateFunction::Avg { distinct, expr } => {
                let mut values = Vec::new();
                for row in rows {
                    match self.evaluate_expr_with_combined_schema(expr, row, schema) {
                        Ok(val) if val != Value::Null => {
                            values.push(val);
                        }
                        _ => {}
                    }
                }
                if values.is_empty() {
                    return Ok(Value::Null);
                }
                if *distinct {
                    let distinct_values: HashSet<Value> = values.iter().cloned().collect();
                    let sum: Result<i64> = distinct_values.iter().map(|v| {
                        match v {
                            Value::Integer(i) => Ok(*i),
                            _ => Err(anyhow!("AVG only works on numeric types")),
                        }
                    }).sum();
                    let count = distinct_values.len() as i64;
                    Ok(Value::Integer(sum? / count))
                } else {
                    let sum: Result<i64> = values.iter().map(|v| {
                        match v {
                            Value::Integer(i) => Ok(*i),
                            _ => Err(anyhow!("AVG only works on numeric types")),
                        }
                    }).sum();
                    let count = values.len() as i64;
                    Ok(Value::Integer(sum? / count))
                }
            }
            crate::parser::ast::AggregateFunction::Min { expr } => {
                let mut min_val: Option<Value> = None;
                for row in rows {
                    match self.evaluate_expr_with_combined_schema(expr, row, schema) {
                        Ok(val) if val != Value::Null => {
                            if let Some(ref current_min) = min_val {
                                if compare_values(&val, current_min)? == std::cmp::Ordering::Less {
                                    min_val = Some(val);
                                }
                            } else {
                                min_val = Some(val);
                            }
                        }
                        _ => {}
                    }
                }
                Ok(min_val.unwrap_or(Value::Null))
            }
            crate::parser::ast::AggregateFunction::Max { expr } => {
                let mut max_val: Option<Value> = None;
                for row in rows {
                    match self.evaluate_expr_with_combined_schema(expr, row, schema) {
                        Ok(val) if val != Value::Null => {
                            if let Some(ref current_max) = max_val {
                                if compare_values(&val, current_max)? == std::cmp::Ordering::Greater {
                                    max_val = Some(val);
                                }
                            } else {
                                max_val = Some(val);
                            }
                        }
                        _ => {}
                    }
                }
                Ok(max_val.unwrap_or(Value::Null))
            }
        }
    }
}
}
