pub mod logical;
pub mod physical;

use anyhow::{anyhow, Result};
use crate::catalog::Catalog;
use crate::parser::ast::*;
use logical::LogicalPlan;
use physical::PhysicalPlan;

/// Query planner that converts AST to execution plans
pub struct Planner {
    // Planner state
}

impl Planner {
    pub fn new() -> Self {
        Self {}
    }

    /// Plan a query from AST
    pub fn plan(&self, stmt: &Statement, catalog: &Catalog) -> Result<PhysicalPlan> {
        match stmt {
            Statement::CreateTable(create) => {
                self.plan_create_table(create, catalog)
            }
            Statement::Select(select) => {
                self.plan_select(select, catalog)
            }
            Statement::Insert(insert) => {
                self.plan_insert(insert, catalog)
            }
            Statement::Update(update) => {
                self.plan_update(update, catalog)
            }
            Statement::Delete(delete) => {
                self.plan_delete(delete, catalog)
            }
        }
    }

    fn plan_create_table(&self, create: &CreateTable, _catalog: &Catalog) -> Result<PhysicalPlan> {
        Ok(PhysicalPlan::CreateTable {
            name: create.name.clone(),
            columns: create.columns.iter().map(|c| (c.name.clone(), c.data_type.clone())).collect(),
        })
    }

    fn plan_select(&self, select: &Select, catalog: &Catalog) -> Result<PhysicalPlan> {
        let from = select.from.as_ref().ok_or_else(|| {
            anyhow!("SELECT statement requires FROM clause")
        })?;

        // Validate all tables in FROM clause exist
        self.validate_table_ref(from, catalog)?;

        // Check if we have aggregate functions
        let has_aggregates = select.columns.iter().any(|item| {
            matches!(item, SelectItem::FunctionCall { .. })
        });

        // If we have GROUP BY, validate that non-aggregate columns are in GROUP BY
        if let Some(group_by) = &select.group_by {
            for item in &select.columns {
                match item {
                    SelectItem::Column(col_name) => {
                        // Handle qualified column names (table.column)
                        let col_name_only = if col_name.contains('.') {
                            col_name.split('.').last().unwrap_or(col_name)
                        } else {
                            col_name
                        };
                        if !group_by.iter().any(|gb_col| {
                            let gb_col_only = if gb_col.contains('.') {
                                gb_col.split('.').last().unwrap_or(gb_col)
                            } else {
                                gb_col
                            };
                            gb_col_only == col_name_only
                        }) {
                            return Err(anyhow!(
                                "Column '{}' must appear in GROUP BY clause or be used in an aggregate function",
                                col_name
                            ));
                        }
                    }
                    SelectItem::All => {
                        if has_aggregates {
                            return Err(anyhow!("Cannot use SELECT * with aggregate functions and GROUP BY"));
                        }
                    }
                    SelectItem::FunctionCall { .. } => {
                        // Aggregate functions are allowed
                    }
                }
            }
        } else if has_aggregates {
            // If we have aggregates but no GROUP BY, all non-aggregate columns must be constants or invalid
            for item in &select.columns {
                match item {
                    SelectItem::Column(_) => {
                        return Err(anyhow!(
                            "Column must appear in GROUP BY clause or be used in an aggregate function"
                        ));
                    }
                    SelectItem::All => {
                        return Err(anyhow!("Cannot use SELECT * with aggregate functions"));
                    }
                    SelectItem::FunctionCall { .. } => {
                        // Aggregate functions are allowed
                    }
                }
            }
        }

        Ok(PhysicalPlan::Select {
            from: from.clone(),
            columns: select.columns.clone(),
            filter: select.where_clause.clone(),
            group_by: select.group_by.clone(),
            limit: select.limit,
        })
    }

    /// Validate that all tables in a TableRef exist in the catalog
    fn validate_table_ref(&self, table_ref: &TableRef, catalog: &Catalog) -> Result<()> {
        match table_ref {
            TableRef::Table { name, .. } => {
                catalog.get_table(name)?;
                Ok(())
            }
            TableRef::Join { left, right, condition, .. } => {
                self.validate_table_ref(left, catalog)?;
                self.validate_table_ref(right, catalog)?;
                // Validate JOIN condition references valid columns
                if let Some(JoinCondition::On(expr)) = condition {
                    self.validate_join_condition(expr, left, right, catalog)?;
                }
                Ok(())
            }
        }
    }

    /// Validate JOIN condition references valid columns
    fn validate_join_condition(
        &self,
        expr: &Expr,
        left: &TableRef,
        right: &TableRef,
        catalog: &Catalog,
    ) -> Result<()> {
        match expr {
            Expr::Column(_) => {
                // Unqualified column - could be ambiguous, but we'll allow it
                Ok(())
            }
            Expr::QualifiedColumn { table, column: _ } => {
                // Verify the table exists in the JOIN (either left or right)
                self.find_table_in_ref(table, left, catalog)
                    .or_else(|_| self.find_table_in_ref(table, right, catalog))
                    .map_err(|_| anyhow!("Table '{}' not found in JOIN", table))
            }
            Expr::Literal(_) => Ok(()),
            Expr::BinaryOp { left: left_expr, right: right_expr, .. } => {
                self.validate_join_condition(left_expr, left, right, catalog)?;
                self.validate_join_condition(right_expr, left, right, catalog)?;
                Ok(())
            }
        }
    }

    /// Find a table by name or alias in a TableRef
    fn find_table_in_ref(&self, table_name: &str, table_ref: &TableRef, catalog: &Catalog) -> Result<()> {
        match table_ref {
            TableRef::Table { name, alias } => {
                if name == table_name || alias.as_ref().map(|a| a == table_name).unwrap_or(false) {
                    catalog.get_table(name)?;
                    Ok(())
                } else {
                    Err(anyhow!("Table '{}' not found in JOIN", table_name))
                }
            }
            TableRef::Join { left, right, .. } => {
                // Try left side first, then right side
                self.find_table_in_ref(table_name, left, catalog)
                    .or_else(|_| self.find_table_in_ref(table_name, right, catalog))
            }
        }
    }

    fn plan_insert(&self, insert: &Insert, catalog: &Catalog) -> Result<PhysicalPlan> {
        // Verify table exists
        catalog.get_table(&insert.table)?;

        Ok(PhysicalPlan::Insert {
            table: insert.table.clone(),
            columns: insert.columns.clone(),
            values: insert.values.clone(),
        })
    }

    fn plan_update(&self, update: &Update, catalog: &Catalog) -> Result<PhysicalPlan> {
        // Verify table exists
        let table = catalog.get_table(&update.table)?;

        // Map column names to indices
        let mut assignments = Vec::new();
        for assign in &update.set {
            let col = table.get_column(&assign.column).ok_or_else(|| {
                anyhow!("Column '{}' not found in table '{}'", assign.column, update.table)
            })?;
            assignments.push((col.ordinal, assign.value.clone()));
        }

        Ok(PhysicalPlan::Update {
            table: update.table.clone(),
            assignments,
            filter: update.where_clause.clone(),
        })
    }

    fn plan_delete(&self, delete: &Delete, catalog: &Catalog) -> Result<PhysicalPlan> {
        // Verify table exists
        catalog.get_table(&delete.table)?;

        Ok(PhysicalPlan::Delete {
            table: delete.table.clone(),
            filter: delete.where_clause.clone(),
        })
    }
}

impl Default for Planner {
    fn default() -> Self {
        Self::new()
    }
}

