use anyhow::{anyhow, Result};
use sqlparser::ast::{
    Assignment as SqlAssignment, BinaryOperator as SqlBinaryOperator, Expr as SqlExpr,
    JoinOperator, SelectItem as SqlSelectItem, Statement as SqlStatement, TableFactor,
    TableWithJoins, Values as SqlValues,
};

use crate::catalog::types::DataType;
use crate::parser::ast::*;
use crate::Value;

/// Convert SQLParser AST to our internal AST
pub fn convert_statement(stmt: SqlStatement) -> Result<Statement> {
    match stmt {
        SqlStatement::CreateTable { name, columns, .. } => {
            let table_name = normalize_identifier(name.to_string());
            let mut column_defs = Vec::new();

            for col in columns {
                let col_name = normalize_identifier(col.name.to_string());
                let data_type = convert_data_type(&col.data_type)?;
                column_defs.push(ColumnDef {
                    name: col_name,
                    data_type,
                });
            }

            Ok(Statement::CreateTable(CreateTable {
                name: table_name,
                columns: column_defs,
            }))
        }
        SqlStatement::Query(query) => {
            let select = match query.body.as_ref() {
                sqlparser::ast::SetExpr::Select(select) => select.as_ref(),
                _ => return Err(anyhow!("Expected SELECT query")),
            };

            let mut select_items = Vec::new();
            for item in &select.projection {
                match item {
                    SqlSelectItem::UnnamedExpr(expr) => {
                        match expr {
                            SqlExpr::Identifier(ident) => {
                                select_items.push(SelectItem::Column(
                                    normalize_identifier(ident.to_string()),
                                ));
                            }
                            SqlExpr::CompoundIdentifier(parts) => {
                                // Qualified column name: table.column
                                if parts.len() == 2 {
                                    let table = normalize_identifier(parts[0].to_string());
                                    let column = normalize_identifier(parts[1].to_string());
                                    select_items.push(SelectItem::Column(format!("{}.{}", table, column)));
                                } else {
                                    return Err(anyhow!("Unsupported compound identifier: {:?}", parts));
                                }
                            }
                            SqlExpr::Function(function) => {
                                let func_name = normalize_identifier(function.name.to_string());
                                let aggregate_func = convert_aggregate_function(&func_name, function)?;
                                select_items.push(SelectItem::FunctionCall {
                                    name: func_name,
                                    function: aggregate_func,
                                });
                            }
                            _ => {
                                return Err(anyhow!("Complex expressions not yet supported: {:?}", expr));
                            }
                        }
                    }
                    SqlSelectItem::Wildcard(_) => {
                        select_items.push(SelectItem::All);
                    }
                    _ => return Err(anyhow!("Unsupported SELECT item")),
                }
            }

            let from = if let Some(from) = &select.from.first() {
                Some(convert_table_with_joins(from)?)
            } else {
                None
            };

            let where_clause = select
                .selection
                .as_ref()
                .map(|expr| convert_expr(expr))
                .transpose()?;

            // Parse GROUP BY clause
            // In sqlparser 0.40, group_by might be structured differently
            // Let's check if it's a vector or something else
            let group_by = match &select.group_by {
                sqlparser::ast::GroupByExpr::All => {
                    return Err(anyhow!("GROUP BY ALL not supported"));
                }
                sqlparser::ast::GroupByExpr::Expressions(exprs) => {
                    if exprs.is_empty() {
                        None
                    } else {
                        Some(
                            exprs
                                .iter()
                                .map(|expr| {
                                    if let SqlExpr::Identifier(ident) = expr {
                                        Ok(normalize_identifier(ident.to_string()))
                                    } else {
                                        Err(anyhow!("GROUP BY only supports column names"))
                                    }
                                })
                                .collect::<Result<Vec<_>>>()?,
                        )
                    }
                }
            };

            // Parse LIMIT clause (Snowflake supports LIMIT n)
            let limit = query.limit.as_ref().and_then(|limit_expr| {
                // LIMIT can be an expression, but we'll handle simple integer literals
                if let SqlExpr::Value(sqlparser::ast::Value::Number(n, _)) = limit_expr {
                    n.parse::<u64>().ok()
                } else {
                    None
                }
            });

            Ok(Statement::Select(Select {
                columns: select_items,
                from,
                where_clause,
                group_by,
                limit,
            }))
        }
        SqlStatement::Insert {
            table_name,
            columns,
            source,
            ..
        } => {
            let table = normalize_identifier(table_name.to_string());
            let column_names: Vec<String> = columns
                .iter()
                .map(|c| normalize_identifier(c.to_string()))
                .collect();

            let values = if let Some(query) = source.as_ref() {
                if let sqlparser::ast::SetExpr::Values(SqlValues { rows, .. }) = query.body.as_ref() {
                    rows.iter()
                        .map(|row| {
                            row.iter()
                                .map(|expr| convert_expr(expr))
                                .collect::<Result<Vec<_>>>()
                        })
                        .collect::<Result<Vec<_>>>()?
                } else {
                    return Err(anyhow!("INSERT with SELECT not yet supported"));
                }
            } else {
                return Err(anyhow!("INSERT source is required"));
            };

            Ok(Statement::Insert(Insert {
                table,
                columns: column_names,
                values,
            }))
        }
        SqlStatement::Update {
            table,
            assignments,
            from,
            selection,
            ..
        } => {
            let table_name = normalize_identifier(table.to_string());
            let set: Vec<Assignment> = assignments
                .iter()
                .map(|a| convert_assignment(a))
                .collect::<Result<Vec<_>>>()?;

            let where_clause = selection
                .as_ref()
                .map(|expr| convert_expr(expr))
                .transpose()?;

            Ok(Statement::Update(Update {
                table: table_name,
                set,
                where_clause,
            }))
        }
        SqlStatement::Delete {
            tables,
            using,
            selection,
            from,
            ..
        } => {
            // DELETE FROM table - table can be in `tables`, `using`, or `from`
            let table = if !tables.is_empty() {
                normalize_identifier(tables[0].to_string())
            } else if let Some(from_table) = from.first() {
                match &from_table.relation {
                    TableFactor::Table { name, .. } => {
                        normalize_identifier(name.to_string())
                    }
                    _ => return Err(anyhow!("Unsupported DELETE table reference")),
                }
            } else if let Some(using_vec) = using.as_ref() {
                if let Some(using_table) = using_vec.first() {
                    match &using_table.relation {
                        TableFactor::Table { name, .. } => {
                            normalize_identifier(name.to_string())
                        }
                        _ => return Err(anyhow!("Unsupported DELETE table reference")),
                    }
                } else {
                    return Err(anyhow!("DELETE requires a table name"));
                }
            } else {
                return Err(anyhow!("DELETE requires a table name"));
            };
            let where_clause = selection
                .as_ref()
                .map(|expr| convert_expr(expr))
                .transpose()?;

            Ok(Statement::Delete(Delete {
                table,
                where_clause,
            }))
        }
        _ => Err(anyhow!("Unsupported statement type")),
    }
}

fn convert_data_type(sql_type: &sqlparser::ast::DataType) -> Result<DataType> {
    // Check Debug string first to handle Integer(None) and other edge cases
    let type_str = format!("{:?}", sql_type);
    
    // Handle integer types (including Integer(None))
    if type_str.starts_with("Int(") || type_str.starts_with("BigInt(") || type_str.starts_with("Integer(") {
        return Ok(DataType::Integer);
    }
    
    // Handle string types
    if type_str.starts_with("Varchar(") || type_str.starts_with("String(") || type_str.starts_with("Char(") || type_str.starts_with("Character(") {
        return Ok(DataType::Varchar);
    }
    
    // Try pattern matching as fallback
    match sql_type {
        sqlparser::ast::DataType::Int(_) 
        | sqlparser::ast::DataType::BigInt(_)
        | sqlparser::ast::DataType::Integer(_) => {
            Ok(DataType::Integer)
        }
        sqlparser::ast::DataType::Varchar(_) 
        | sqlparser::ast::DataType::String(_)
        | sqlparser::ast::DataType::Char(_)
        | sqlparser::ast::DataType::Character(_) => {
            Ok(DataType::Varchar)
        }
        sqlparser::ast::DataType::Boolean => Ok(DataType::Boolean),
        _ => Err(anyhow!("Unsupported data type: {:?}", sql_type)),
    }
}

fn convert_expr(expr: &SqlExpr) -> Result<Expr> {
    match expr {
        SqlExpr::Identifier(ident) => Ok(Expr::Column(normalize_identifier(ident.to_string()))),
        SqlExpr::CompoundIdentifier(parts) => {
            // Qualified column name: table.column
            if parts.len() == 2 {
                let table = normalize_identifier(parts[0].to_string());
                let column = normalize_identifier(parts[1].to_string());
                Ok(Expr::QualifiedColumn { table, column })
            } else {
                Err(anyhow!("Unsupported compound identifier: {:?}", parts))
            }
        }
        SqlExpr::Value(val) => Ok(Expr::Literal(convert_value(val)?)),
        SqlExpr::BinaryOp { left, op, right } => {
            let left_expr = convert_expr(left)?;
            let right_expr = convert_expr(right)?;
            let op = convert_binary_op(op)?;
            Ok(Expr::BinaryOp {
                left: Box::new(left_expr),
                op,
                right: Box::new(right_expr),
            })
        }
        _ => Err(anyhow!("Unsupported expression: {:?}", expr)),
    }
}

fn convert_binary_op(op: &SqlBinaryOperator) -> Result<BinaryOperator> {
    match op {
        SqlBinaryOperator::Eq => Ok(BinaryOperator::Eq),
        SqlBinaryOperator::NotEq => Ok(BinaryOperator::Ne),
        SqlBinaryOperator::Lt => Ok(BinaryOperator::Lt),
        SqlBinaryOperator::Gt => Ok(BinaryOperator::Gt),
        SqlBinaryOperator::LtEq => Ok(BinaryOperator::Le),
        SqlBinaryOperator::GtEq => Ok(BinaryOperator::Ge),
        SqlBinaryOperator::And => Ok(BinaryOperator::And),
        SqlBinaryOperator::Or => Ok(BinaryOperator::Or),
        _ => Err(anyhow!("Unsupported binary operator: {:?}", op)),
    }
}

fn convert_value(val: &sqlparser::ast::Value) -> Result<Value> {
    match val {
        sqlparser::ast::Value::Number(n, _) => {
            Ok(Value::Integer(n.parse().map_err(|e| anyhow!("Invalid number: {}", e))?))
        }
        sqlparser::ast::Value::SingleQuotedString(s) | sqlparser::ast::Value::DoubleQuotedString(s) => {
            Ok(Value::Varchar(s.clone()))
        }
        sqlparser::ast::Value::Boolean(b) => Ok(Value::Boolean(*b)),
        sqlparser::ast::Value::Null => Ok(Value::Null),
        _ => Err(anyhow!("Unsupported value type: {:?}", val)),
    }
}

fn convert_assignment(assign: &SqlAssignment) -> Result<Assignment> {
    let column = if assign.id.len() == 1 {
        normalize_identifier(assign.id[0].to_string())
    } else {
        return Err(anyhow!("Multi-part column names not supported"));
    };
    let value = convert_expr(&assign.value)?;
    Ok(Assignment { column, value })
}

/// Convert aggregate function from sqlparser
fn convert_aggregate_function(
    name: &str,
    function: &sqlparser::ast::Function,
) -> Result<AggregateFunction> {
    let distinct = function.distinct;
    
    match name {
        "COUNT" => {
            if function.args.is_empty() {
                // COUNT(*)
                Ok(AggregateFunction::Count {
                    distinct: false,
                    expr: None,
                })
            } else if function.args.len() == 1 {
                // COUNT(expr) or COUNT(DISTINCT expr)
                let arg = &function.args[0];
                match arg {
                    sqlparser::ast::FunctionArg::Unnamed(sqlparser::ast::FunctionArgExpr::Wildcard) => {
                        // COUNT(*)
                        Ok(AggregateFunction::Count {
                            distinct: false,
                            expr: None,
                        })
                    }
                    sqlparser::ast::FunctionArg::Unnamed(sqlparser::ast::FunctionArgExpr::Expr(expr)) => {
                        // COUNT(column) or COUNT(DISTINCT column)
                        Ok(AggregateFunction::Count {
                            distinct,
                            expr: Some(Box::new(convert_expr(expr)?)),
                        })
                    }
                    _ => Err(anyhow!("Unsupported COUNT argument")),
                }
            } else {
                Err(anyhow!("COUNT() takes at most one argument"))
            }
        }
        "SUM" => {
            if function.args.len() != 1 {
                return Err(anyhow!("SUM() requires exactly one argument"));
            }
            let arg = &function.args[0];
            match arg {
                sqlparser::ast::FunctionArg::Unnamed(sqlparser::ast::FunctionArgExpr::Expr(expr)) => {
                    Ok(AggregateFunction::Sum {
                        distinct,
                        expr: Box::new(convert_expr(expr)?),
                    })
                }
                _ => Err(anyhow!("Unsupported SUM argument")),
            }
        }
        "AVG" => {
            if function.args.len() != 1 {
                return Err(anyhow!("AVG() requires exactly one argument"));
            }
            let arg = &function.args[0];
            match arg {
                sqlparser::ast::FunctionArg::Unnamed(sqlparser::ast::FunctionArgExpr::Expr(expr)) => {
                    Ok(AggregateFunction::Avg {
                        distinct,
                        expr: Box::new(convert_expr(expr)?),
                    })
                }
                _ => Err(anyhow!("Unsupported AVG argument")),
            }
        }
        "MIN" => {
            if function.args.len() != 1 {
                return Err(anyhow!("MIN() requires exactly one argument"));
            }
            let arg = &function.args[0];
            match arg {
                sqlparser::ast::FunctionArg::Unnamed(sqlparser::ast::FunctionArgExpr::Expr(expr)) => {
                    Ok(AggregateFunction::Min {
                        expr: Box::new(convert_expr(expr)?),
                    })
                }
                _ => Err(anyhow!("Unsupported MIN argument")),
            }
        }
        "MAX" => {
            if function.args.len() != 1 {
                return Err(anyhow!("MAX() requires exactly one argument"));
            }
            let arg = &function.args[0];
            match arg {
                sqlparser::ast::FunctionArg::Unnamed(sqlparser::ast::FunctionArgExpr::Expr(expr)) => {
                    Ok(AggregateFunction::Max {
                        expr: Box::new(convert_expr(expr)?),
                    })
                }
                _ => Err(anyhow!("Unsupported MAX argument")),
            }
        }
        _ => Err(anyhow!("Unsupported aggregate function: {}", name)),
    }
}

/// Convert TableWithJoins from sqlparser to our TableRef
fn convert_table_with_joins(table_with_joins: &TableWithJoins) -> Result<TableRef> {
    // Convert the base table
    let mut current_table = match &table_with_joins.relation {
        TableFactor::Table { name, alias, .. } => {
            let table_name = normalize_identifier(name.to_string());
            let table_alias = alias.as_ref().map(|a| normalize_identifier(a.name.to_string()));
            TableRef::Table {
                name: table_name,
                alias: table_alias,
            }
        }
        _ => return Err(anyhow!("Unsupported table factor in FROM clause")),
    };

    // Process JOINs
    for join in &table_with_joins.joins {
        let right_table = match &join.relation {
            TableFactor::Table { name, alias, .. } => {
                let table_name = normalize_identifier(name.to_string());
                let table_alias = alias.as_ref().map(|a| normalize_identifier(a.name.to_string()));
                TableRef::Table {
                    name: table_name,
                    alias: table_alias,
                }
            }
            _ => return Err(anyhow!("Unsupported table factor in JOIN")),
        };

        // Convert JOIN type
        let join_type = match &join.join_operator {
            JoinOperator::Inner(_) => JoinType::Inner,
            JoinOperator::LeftOuter(_) => JoinType::Left,
            JoinOperator::RightOuter(_) => JoinType::Right,
            JoinOperator::FullOuter(_) => JoinType::FullOuter,
            JoinOperator::CrossJoin => JoinType::Cross,
            _ => return Err(anyhow!("Unsupported JOIN type: {:?}", join.join_operator)),
        };

        // Convert JOIN condition
        let condition = match &join.join_operator {
            JoinOperator::Inner(condition) | 
            JoinOperator::LeftOuter(condition) | 
            JoinOperator::RightOuter(condition) | 
            JoinOperator::FullOuter(condition) => {
                match condition {
                    sqlparser::ast::JoinConstraint::On(expr) => {
                        Some(JoinCondition::On(convert_expr(expr)?))
                    }
                    sqlparser::ast::JoinConstraint::Using(columns) => {
                        let cols: Result<Vec<String>> = columns
                            .iter()
                            .map(|c| Ok(normalize_identifier(c.to_string())))
                            .collect();
                        Some(JoinCondition::Using(cols?))
                    }
                    sqlparser::ast::JoinConstraint::Natural => {
                        return Err(anyhow!("NATURAL JOIN not yet supported"));
                    }
                    sqlparser::ast::JoinConstraint::None => None,
                }
            }
            JoinOperator::CrossJoin => None,
            _ => None,
        };

        // Build the JOIN
        current_table = TableRef::Join {
            left: Box::new(current_table),
            right: Box::new(right_table),
            join_type,
            condition,
        };
    }

    Ok(current_table)
}

/// Normalize identifier (remove quotes, handle case)
fn normalize_identifier(ident: String) -> String {
    // Remove surrounding quotes if present
    let trimmed = ident.trim();
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        ident.to_uppercase() // Snowflake is case-insensitive, store uppercase
    }
}
