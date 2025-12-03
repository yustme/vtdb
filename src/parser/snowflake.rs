use anyhow::{anyhow, Result};
use sqlparser::ast::{
    Assignment as SqlAssignment, BinaryOperator as SqlBinaryOperator, Expr as SqlExpr,
    SelectItem as SqlSelectItem, Statement as SqlStatement, TableFactor,
    Values as SqlValues,
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
                        if let SqlExpr::Identifier(ident) = expr {
                            select_items.push(SelectItem::Column(
                                normalize_identifier(ident.to_string()),
                            ));
                        } else {
                            return Err(anyhow!("Complex expressions not yet supported"));
                        }
                    }
                    SqlSelectItem::Wildcard(_) => {
                        select_items.push(SelectItem::All);
                    }
                    _ => return Err(anyhow!("Unsupported SELECT item")),
                }
            }

            let from = if let Some(from) = &select.from.first() {
                match &from.relation {
                    TableFactor::Table { name, .. } => {
                        Some(TableRef {
                            name: normalize_identifier(name.to_string()),
                        })
                    }
                    _ => return Err(anyhow!("Unsupported FROM clause")),
                }
            } else {
                None
            };

            let where_clause = select
                .selection
                .as_ref()
                .map(|expr| convert_expr(expr))
                .transpose()?;

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
