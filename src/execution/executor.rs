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
            PhysicalPlan::Select { table, columns, filter } => {
                self.execute_select(table, columns, filter, storage, catalog)
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

        // Project columns
        let result_rows = if columns.is_empty() || matches!(columns[0], crate::parser::ast::SelectItem::All) {
            // SELECT *
            filtered_rows
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
                }
            }

            filtered_rows
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

