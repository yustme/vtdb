use anyhow::{anyhow, Result};
use crate::catalog::Catalog;
use crate::planner::physical::PhysicalPlan;
use crate::storage::StorageEngine;
use crate::Value;
use crate::QueryResult;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

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

/// Join key information for hash joins
#[derive(Clone)]
struct JoinKeyInfo {
    left_indices: Vec<usize>,
    right_indices: Vec<usize>,
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
        self.execute_with_progress(plan, storage, catalog, None, None)
    }

    /// Execute with optional progress tracking
    pub fn execute_with_progress(
        &self,
        plan: &PhysicalPlan,
        storage: &mut StorageEngine,
        catalog: &mut Catalog,
        active_queries: Option<Arc<Mutex<HashMap<String, crate::QueryProgressTracker>>>>,
        query_id: Option<&str>,
    ) -> Result<QueryResult> {
        match plan {
            PhysicalPlan::CreateTable { name, columns } => {
                self.execute_create_table(name, columns, storage, catalog)
            }
            PhysicalPlan::Select { from, columns, filter, group_by, limit } => {
                self.execute_select_with_progress(from, columns, filter, group_by, limit, storage, catalog, active_queries, query_id)
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
            PhysicalPlan::DropAllTables => {
                self.execute_drop_all_tables(storage, catalog)
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

        // Create in storage (with Iceberg support)
        storage.create_table_with_schema(name.to_string(), column_defs)?;

        // Automatically create indexes on all columns (start with hash indexes)
        let index_manager = storage.index_manager_mut();
        for (col_idx, (col_name, _)) in columns.iter().enumerate() {
            index_manager.create_index(name, col_name, col_idx, crate::index::IndexType::Hash);
        }

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
        storage: &mut StorageEngine,
        catalog: &Catalog,
    ) -> Result<QueryResult> {
        // Try to use index scan for single table queries with filters
        let (all_rows, combined_schema) = match (from, filter) {
            (crate::parser::ast::TableRef::Table { name, alias }, Some(filter_expr)) => {
                // Check if we can use an index
                if let Some((column_name, key_value)) = self.extract_equality_predicate(filter_expr) {
                    // Flush buffer before index scan to ensure latest data is visible
                    if storage.has_pending_buffer_data(name) {
                        if let Err(e) = storage.flush_table_iceberg_writes(name) {
                            eprintln!("Warning: Failed to flush buffer before index scan for table {}: {}", name, e);
                        }
                    }
                    // Try to use index for equality lookup
                    use crate::execution::operators::IndexScan;
                    let row_ids = IndexScan::scan_by_key(name, &column_name, key_value, storage)?;
                    if !row_ids.is_empty() {
                        // Use index scan
                        let rows = IndexScan::get_rows_by_ids(name, &row_ids, storage)?;
                        let schema = catalog.get_table(name)?;
                        let combined_schema = CombinedSchema::from_single_table(schema, alias.clone())?;
                        return self.finish_select_execution(rows, &combined_schema, columns, filter, group_by, limit);
                    }
                }
                // Fall back to regular scan
                self.execute_table_ref(from, storage, catalog)?
            }
            _ => {
                // Execute JOINs or single table scan without filter
                self.execute_table_ref(from, storage, catalog)?
            }
        };

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

        self.finish_select_execution(filtered_rows, &combined_schema, columns, filter, group_by, limit)
    }

    /// Execute SELECT query with progress tracking (public method)
    pub fn execute_select_with_progress_direct(
        &self,
        plan: &PhysicalPlan,
        storage: &mut StorageEngine,
        catalog: &Catalog,
        active_queries: Option<Arc<Mutex<HashMap<String, crate::QueryProgressTracker>>>>,
        query_id: Option<&str>,
    ) -> Result<QueryResult> {
        match plan {
            PhysicalPlan::Select { from, columns, filter, group_by, limit } => {
                self.execute_select_with_progress(from, columns, filter, group_by, limit, storage, catalog, active_queries, query_id)
            }
            _ => Err(anyhow::anyhow!("This method only supports SELECT queries"))
        }
    }

    /// Execute SELECT with progress tracking
    fn execute_select_with_progress(
        &self,
        from: &crate::parser::ast::TableRef,
        columns: &[crate::parser::ast::SelectItem],
        filter: &Option<crate::parser::ast::Expr>,
        group_by: &Option<Vec<String>>,
        limit: &Option<u64>,
        storage: &mut StorageEngine,
        catalog: &Catalog,
        active_queries: Option<Arc<Mutex<HashMap<String, crate::QueryProgressTracker>>>>,
        query_id: Option<&str>,
    ) -> Result<QueryResult> {
        const CHUNK_SIZE: usize = 10000;

        // If no progress tracking, use regular execution
        if active_queries.is_none() || query_id.is_none() {
            return self.execute_select(from, columns, filter, group_by, limit, storage, catalog);
        }

        let active_queries = active_queries.unwrap();
        let query_id = query_id.unwrap();

        // Try to use index scan for single table queries with filters
        let (all_rows, combined_schema) = match (from, filter) {
            (crate::parser::ast::TableRef::Table { name, alias }, Some(filter_expr)) => {
                // Check if we can use an index
                if let Some((column_name, key_value)) = self.extract_equality_predicate(filter_expr) {
                    // Flush buffer before index scan to ensure latest data is visible
                    if storage.has_pending_buffer_data(name) {
                        if let Err(e) = storage.flush_table_iceberg_writes(name) {
                            eprintln!("Warning: Failed to flush buffer before index scan for table {}: {}", name, e);
                        }
                    }
                    // Try to use index for equality lookup
                    use crate::execution::operators::IndexScan;
                    let row_ids = IndexScan::scan_by_key(name, &column_name, key_value, storage)?;
                    if !row_ids.is_empty() {
                        // Use index scan - no progress tracking needed for indexed lookups
                        let rows = IndexScan::get_rows_by_ids(name, &row_ids, storage)?;
                        let schema = catalog.get_table(name)?;
                        let combined_schema = CombinedSchema::from_single_table(schema, alias.clone())?;
                        return self.finish_select_execution(rows, &combined_schema, columns, filter, group_by, limit);
                    }
                }
                // Fall back to chunked scan
                self.execute_table_ref_with_progress(from, storage, catalog, &active_queries, query_id, CHUNK_SIZE)?
            }
            _ => {
                // Execute JOINs or single table scan with progress
                self.execute_table_ref_with_progress(from, storage, catalog, &active_queries, query_id, CHUNK_SIZE)?
            }
        };

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

        self.finish_select_execution(filtered_rows, &combined_schema, columns, filter, group_by, limit)
    }

    /// Finish SELECT execution (common code for both index and table scans)
    fn finish_select_execution(
        &self,
        filtered_rows: Vec<Vec<Value>>,
        combined_schema: &CombinedSchema,
        columns: &[crate::parser::ast::SelectItem],
        _filter: &Option<crate::parser::ast::Expr>,
        group_by: &Option<Vec<String>>,
        limit: &Option<u64>,
    ) -> Result<QueryResult> {

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

    /// Extract equality predicate (column = value) for index lookup
    fn extract_equality_predicate(&self, expr: &crate::parser::ast::Expr) -> Option<(String, Value)> {
        match expr {
            crate::parser::ast::Expr::BinaryOp { left, op, right } => {
                if *op == crate::parser::ast::BinaryOperator::Eq {
                    match (left.as_ref(), right.as_ref()) {
                        (crate::parser::ast::Expr::Column(col), crate::parser::ast::Expr::Literal(val)) => {
                            Some((col.clone(), val.clone()))
                        }
                        (crate::parser::ast::Expr::QualifiedColumn { table: _, column }, crate::parser::ast::Expr::Literal(val)) => {
                            Some((column.clone(), val.clone()))
                        }
                        (crate::parser::ast::Expr::Literal(val), crate::parser::ast::Expr::Column(col)) => {
                            Some((col.clone(), val.clone()))
                        }
                        (crate::parser::ast::Expr::Literal(val), crate::parser::ast::Expr::QualifiedColumn { table: _, column }) => {
                            Some((column.clone(), val.clone()))
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Execute a TableRef (table or JOIN) and return rows with combined schema
    fn execute_table_ref(
        &self,
        table_ref: &crate::parser::ast::TableRef,
        storage: &mut StorageEngine,
        catalog: &Catalog,
    ) -> Result<(Vec<Vec<Value>>, CombinedSchema)> {
        match table_ref {
            crate::parser::ast::TableRef::Table { name, alias } => {
                let schema = catalog.get_table(name)?;
                // Try to use index scan if filter is present and indexable
                // For now, we'll scan all rows - index optimization will be added in execute_select
                let rows = storage.scan_table(name)?;
                let combined_schema = CombinedSchema::from_single_table(schema, alias.clone())?;
                Ok((rows, combined_schema))
            }
            crate::parser::ast::TableRef::Join { left, right, join_type, condition } => {
                self.execute_join(left, right, join_type, condition, storage, catalog)
            }
        }
    }

    /// Execute a TableRef with progress tracking using chunked scans
    fn execute_table_ref_with_progress(
        &self,
        table_ref: &crate::parser::ast::TableRef,
        storage: &mut StorageEngine,
        catalog: &Catalog,
        active_queries: &Arc<Mutex<HashMap<String, crate::QueryProgressTracker>>>,
        query_id: &str,
        chunk_size: usize,
    ) -> Result<(Vec<Vec<Value>>, CombinedSchema)> {
        match table_ref {
            crate::parser::ast::TableRef::Table { name, alias } => {
                let schema = catalog.get_table(name)?;
                
                // Get total row count for progress tracking
                let total_rows = storage.get_row_count(name).unwrap_or(0);
                
                // Update progress tracker
                if let Ok(mut queries) = active_queries.lock() {
                    if let Some(tracker) = queries.get_mut(query_id) {
                        tracker.update_table_progress(name.clone(), 0, total_rows);
                        tracker.estimated_total_rows = total_rows;
                        tracker.set_stage(format!("Scanning table {}", name));
                    }
                }

                // Scan table in chunks
                let mut all_rows = Vec::new();
                let mut start_idx = 0;
                
                while start_idx < total_rows {
                    // Check if query was cancelled
                    if let Ok(queries) = active_queries.lock() {
                        if let Some(tracker) = queries.get(query_id) {
                            if tracker.status == crate::QueryStatus::Cancelled {
                                return Err(anyhow::anyhow!("Query cancelled"));
                            }
                        }
                    }

                    // Scan chunk
                    let chunk = storage.scan_table_chunk(name, start_idx, chunk_size)?;
                    all_rows.extend(chunk);
                    start_idx += chunk_size;

                    // Update progress
                    if let Ok(mut queries) = active_queries.lock() {
                        if let Some(tracker) = queries.get_mut(query_id) {
                            tracker.update_table_progress(name.clone(), start_idx.min(total_rows), total_rows);
                            tracker.rows_processed = start_idx.min(total_rows);
                        }
                    }

                    // Yield to other tasks periodically (every chunk)
                    std::thread::yield_now();
                }

                let combined_schema = CombinedSchema::from_single_table(schema, alias.clone())?;
                Ok((all_rows, combined_schema))
            }
            crate::parser::ast::TableRef::Join { left, right, join_type, condition } => {
                // For JOINs, recursively scan with progress
                let (left_rows, left_schema) = self.execute_table_ref_with_progress(
                    left, storage, catalog, active_queries, query_id, chunk_size
                )?;
                let (right_rows, right_schema) = self.execute_table_ref_with_progress(
                    right, storage, catalog, active_queries, query_id, chunk_size
                )?;
                
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
        }
    }

    /// Execute a JOIN operation
    fn execute_join(
        &self,
        left: &crate::parser::ast::TableRef,
        right: &crate::parser::ast::TableRef,
        join_type: &crate::parser::ast::JoinType,
        condition: &Option<crate::parser::ast::JoinCondition>,
        storage: &mut StorageEngine,
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
        let expected_column_count = table_schema.columns.len();
        
        // Evaluate expressions to get values
        let mut rows = Vec::new();
        for value_exprs in values {
            let mut row = Vec::new();
            for expr in value_exprs {
                let value = evaluate_expr(expr, &[], table_schema)?;
                row.push(value);
            }
            
            // Validate that we don't have too many values
            if row.len() > expected_column_count {
                return Err(anyhow::anyhow!(
                    "Row has {} values, but table has {} columns",
                    row.len(),
                    expected_column_count
                ));
            }
            
            // Pad row with NULL values if fewer values provided than columns
            // This allows partial column inserts like INSERT INTO table VALUES (1)
            while row.len() < expected_column_count {
                row.push(Value::Null);
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

    fn execute_drop_all_tables(
        &self,
        storage: &mut StorageEngine,
        catalog: &mut Catalog,
    ) -> Result<QueryResult> {
        storage.drop_all_tables()?;
        catalog.drop_all_tables()?;

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
    }
}

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
        match condition {
            Some(crate::parser::ast::JoinCondition::On(expr)) => {
                // Try hash join for equi-joins
                if let Some(key_info) = self.extract_equi_join_keys(expr, left_schema, right_schema) {
                    // Use hash join for equi-join
                    return Ok(self.hash_join_equi(left_rows, right_rows, &key_info));
                }
                
                // Fall back to nested loop for complex conditions
                let mut result = Vec::new();
                // Pre-allocate with estimated capacity
                let estimated_size = left_rows.len().min(right_rows.len());
                result.reserve(estimated_size);
                
                for left_row in left_rows {
                    for right_row in right_rows {
                        let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                        if self.evaluate_join_condition(expr, &combined_row, left_schema, right_schema)? {
                            result.push(combined_row);
                        }
                    }
                }
                Ok(result)
            }
            Some(crate::parser::ast::JoinCondition::Using(columns)) => {
                // USING clause - equi-join, use hash join
                // Pre-compute column indices
                let mut left_indices = Vec::new();
                let mut right_indices = Vec::new();
                for col_name in columns {
                    left_indices.push(left_schema.find_column(None, col_name)?);
                    right_indices.push(right_schema.find_column(None, col_name)?);
                }
                
                let key_info = JoinKeyInfo {
                    left_indices,
                    right_indices,
                };
                
                let mut result = self.hash_join_equi(left_rows, right_rows, &key_info);
                
                // For USING, we need to deduplicate columns - only include USING columns once
                // Pre-compute which right columns to skip
                let mut right_skip_indices = std::collections::HashSet::new();
                for col_name in columns {
                    if let Ok(idx) = right_schema.find_column(None, col_name) {
                        right_skip_indices.insert(idx);
                    }
                }
                
                // Rebuild result rows with deduplicated columns
                let mut deduplicated_result = Vec::new();
                for row in result {
                    let left_col_count = left_rows.first().map(|r| r.len()).unwrap_or(0);
                    let mut new_row = Vec::with_capacity(left_col_count + right_rows.first().map(|r| r.len() - right_skip_indices.len()).unwrap_or(0));
                    
                    // Add all left columns
                    new_row.extend_from_slice(&row[..left_col_count]);
                    
                    // Add right columns, skipping USING columns
                    for (idx, val) in row[left_col_count..].iter().enumerate() {
                        if !right_skip_indices.contains(&idx) {
                            new_row.push(val.clone());
                        }
                    }
                    deduplicated_result.push(new_row);
                }
                
                Ok(deduplicated_result)
            }
            None => {
                // No condition - cartesian product (shouldn't happen for INNER JOIN, but handle it)
                let mut result = Vec::with_capacity(left_rows.len() * right_rows.len());
                for left_row in left_rows {
                    for right_row in right_rows {
                        let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                        result.push(combined_row);
                    }
                }
                Ok(result)
            }
        }
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
        use std::collections::HashSet;
        
        let right_null_row: Vec<Value> = vec![Value::Null; right_schema.columns.len()];
        let mut result = Vec::new();
        let mut matched_left_indices = HashSet::new();
        
        match condition {
            Some(crate::parser::ast::JoinCondition::On(expr)) => {
                // Try hash join for equi-joins
                if let Some(key_info) = self.extract_equi_join_keys(expr, left_schema, right_schema) {
                    // Use hash join for equi-join
                    // Build hash table on right (probe side)
                    let hash_table = self.build_hash_table(right_rows, &key_info.right_indices);
                    
                    // Probe with left rows
                    for (left_idx, left_row) in left_rows.iter().enumerate() {
                        let left_key: Vec<Value> = key_info.left_indices.iter().map(|&idx| left_row[idx].clone()).collect();
                        
                        if let Some(right_indices) = hash_table.get(&left_key) {
                            matched_left_indices.insert(left_idx);
                            for &right_idx in right_indices {
                                let right_row = &right_rows[right_idx];
                                let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                                result.push(combined_row);
                            }
                        }
                    }
                } else {
                    // Fall back to nested loop for complex conditions
                    for (left_idx, left_row) in left_rows.iter().enumerate() {
                        let mut matched = false;
                        for right_row in right_rows {
                            let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                            if self.evaluate_join_condition(expr, &combined_row, left_schema, right_schema)? {
                                matched_left_indices.insert(left_idx);
                                result.push(combined_row);
                                matched = true;
                            }
                        }
                        if !matched {
                            let mut combined = left_row.clone();
                            combined.extend(right_null_row.clone());
                            result.push(combined);
                        }
                    }
                    return Ok(result);
                }
            }
            Some(crate::parser::ast::JoinCondition::Using(columns)) => {
                // USING clause - equi-join, use hash join
                // Pre-compute column indices
                let mut left_indices = Vec::new();
                let mut right_indices = Vec::new();
                for col_name in columns {
                    left_indices.push(left_schema.find_column(None, col_name)?);
                    right_indices.push(right_schema.find_column(None, col_name)?);
                }
                
                let key_info = JoinKeyInfo {
                    left_indices,
                    right_indices,
                };
                
                // Build hash table on right
                let hash_table = self.build_hash_table(right_rows, &key_info.right_indices);
                
                // Pre-compute which right columns to skip for USING
                let mut right_skip_indices = std::collections::HashSet::new();
                for col_name in columns {
                    if let Ok(idx) = right_schema.find_column(None, col_name) {
                        right_skip_indices.insert(idx);
                    }
                }
                
                // Probe with left rows
                for (left_idx, left_row) in left_rows.iter().enumerate() {
                    let left_key: Vec<Value> = key_info.left_indices.iter().map(|&idx| left_row[idx].clone()).collect();
                    
                    if let Some(right_row_indices) = hash_table.get(&left_key) {
                        matched_left_indices.insert(left_idx);
                        for &right_idx in right_row_indices {
                            let right_row = &right_rows[right_idx];
                            let mut combined = left_row.clone();
                            // Add right columns, skipping USING columns
                            for (idx, val) in right_row.iter().enumerate() {
                                if !right_skip_indices.contains(&idx) {
                                    combined.push(val.clone());
                                }
                            }
                            result.push(combined);
                        }
                    }
                }
            }
            None => {
                // CROSS JOIN behavior
                let mut result = Vec::with_capacity(left_rows.len() * right_rows.len());
                for left_row in left_rows {
                    for right_row in right_rows {
                        let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                        result.push(combined_row);
                    }
                }
                return Ok(result);
            }
        }
        
        // Add unmatched left rows with NULLs for right
        for (left_idx, left_row) in left_rows.iter().enumerate() {
            if !matched_left_indices.contains(&left_idx) {
                let mut combined = left_row.clone();
                combined.extend(right_null_row.clone());
                result.push(combined);
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
        use std::collections::HashSet;
        
        let left_null_row: Vec<Value> = vec![Value::Null; left_schema.columns.len()];
        let mut result = Vec::new();
        let mut matched_right_indices = HashSet::new();
        
        match condition {
            Some(crate::parser::ast::JoinCondition::On(expr)) => {
                // Try hash join for equi-joins
                if let Some(key_info) = self.extract_equi_join_keys(expr, left_schema, right_schema) {
                    // Use hash join for equi-join
                    // Build hash table on left (probe side)
                    let hash_table = self.build_hash_table(left_rows, &key_info.left_indices);
                    
                    // Probe with right rows
                    for (right_idx, right_row) in right_rows.iter().enumerate() {
                        let right_key: Vec<Value> = key_info.right_indices.iter().map(|&idx| right_row[idx].clone()).collect();
                        
                        if let Some(left_indices) = hash_table.get(&right_key) {
                            matched_right_indices.insert(right_idx);
                            for &left_idx in left_indices {
                                let left_row = &left_rows[left_idx];
                                let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                                result.push(combined_row);
                            }
                        }
                    }
                } else {
                    // Fall back to nested loop for complex conditions
                    for (right_idx, right_row) in right_rows.iter().enumerate() {
                        let mut matched = false;
                        for left_row in left_rows {
                            let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                            if self.evaluate_join_condition(expr, &combined_row, left_schema, right_schema)? {
                                matched_right_indices.insert(right_idx);
                                result.push(combined_row);
                                matched = true;
                            }
                        }
                        if !matched {
                            let mut combined = left_null_row.clone();
                            combined.extend(right_row.clone());
                            result.push(combined);
                        }
                    }
                    return Ok(result);
                }
            }
            Some(crate::parser::ast::JoinCondition::Using(columns)) => {
                // USING clause - equi-join, use hash join
                // Pre-compute column indices
                let mut left_indices = Vec::new();
                let mut right_indices = Vec::new();
                for col_name in columns {
                    left_indices.push(left_schema.find_column(None, col_name)?);
                    right_indices.push(right_schema.find_column(None, col_name)?);
                }
                
                let key_info = JoinKeyInfo {
                    left_indices,
                    right_indices,
                };
                
                // Build hash table on left
                let hash_table = self.build_hash_table(left_rows, &key_info.left_indices);
                
                // Pre-compute which right columns to skip for USING
                let mut right_skip_indices = std::collections::HashSet::new();
                for col_name in columns {
                    if let Ok(idx) = right_schema.find_column(None, col_name) {
                        right_skip_indices.insert(idx);
                    }
                }
                
                // Probe with right rows
                for (right_idx, right_row) in right_rows.iter().enumerate() {
                    let right_key: Vec<Value> = key_info.right_indices.iter().map(|&idx| right_row[idx].clone()).collect();
                    
                    if let Some(left_row_indices) = hash_table.get(&right_key) {
                        matched_right_indices.insert(right_idx);
                        for &left_idx in left_row_indices {
                            let left_row = &left_rows[left_idx];
                            let mut combined = left_row.clone();
                            // Add right columns, skipping USING columns
                            for (idx, val) in right_row.iter().enumerate() {
                                if !right_skip_indices.contains(&idx) {
                                    combined.push(val.clone());
                                }
                            }
                            result.push(combined);
                        }
                    }
                }
            }
            None => {
                // CROSS JOIN behavior
                let mut result = Vec::with_capacity(left_rows.len() * right_rows.len());
                for left_row in left_rows {
                    for right_row in right_rows {
                        let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                        result.push(combined_row);
                    }
                }
                return Ok(result);
            }
        }
        
        // Add unmatched right rows with NULLs for left
        for (right_idx, right_row) in right_rows.iter().enumerate() {
            if !matched_right_indices.contains(&right_idx) {
                let mut combined = left_null_row.clone();
                combined.extend(right_row.clone());
                result.push(combined);
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
                // Try hash join for equi-joins
                if let Some(key_info) = self.extract_equi_join_keys(expr, left_schema, right_schema) {
                    // Use hash join for equi-join
                    // Build hash table on right (smaller table)
                    let hash_table = self.build_hash_table(right_rows, &key_info.right_indices);
                    
                    // Probe with left rows
                    for (left_idx, left_row) in left_rows.iter().enumerate() {
                        let left_key: Vec<Value> = key_info.left_indices.iter().map(|&idx| left_row[idx].clone()).collect();
                        
                        if let Some(right_indices) = hash_table.get(&left_key) {
                            matched_left_indices.insert(left_idx);
                            for &right_idx in right_indices {
                                matched_right_indices.insert(right_idx);
                                let right_row = &right_rows[right_idx];
                                let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                                result.push(combined_row);
                            }
                        }
                    }
                } else {
                    // Fall back to nested loop for complex conditions
                    for (left_idx, left_row) in left_rows.iter().enumerate() {
                        for (right_idx, right_row) in right_rows.iter().enumerate() {
                            let combined_row = [left_row.as_slice(), right_row.as_slice()].concat();
                            if self.evaluate_join_condition(expr, &combined_row, left_schema, right_schema)? {
                                matched_left_indices.insert(left_idx);
                                matched_right_indices.insert(right_idx);
                                result.push(combined_row);
                            }
                        }
                    }
                }
            }
            Some(crate::parser::ast::JoinCondition::Using(columns)) => {
                // USING clause - equi-join, use hash join
                // Pre-compute column indices
                let mut left_indices = Vec::new();
                let mut right_indices = Vec::new();
                for col_name in columns {
                    left_indices.push(left_schema.find_column(None, col_name)?);
                    right_indices.push(right_schema.find_column(None, col_name)?);
                }
                
                let key_info = JoinKeyInfo {
                    left_indices,
                    right_indices,
                };
                
                // Build hash table on right
                let hash_table = self.build_hash_table(right_rows, &key_info.right_indices);
                
                // Pre-compute which right columns to skip for USING
                let mut right_skip_indices = std::collections::HashSet::new();
                for col_name in columns {
                    if let Ok(idx) = right_schema.find_column(None, col_name) {
                        right_skip_indices.insert(idx);
                    }
                }
                
                // Probe with left rows
                for (left_idx, left_row) in left_rows.iter().enumerate() {
                    let left_key: Vec<Value> = key_info.left_indices.iter().map(|&idx| left_row[idx].clone()).collect();
                    
                    if let Some(right_row_indices) = hash_table.get(&left_key) {
                        matched_left_indices.insert(left_idx);
                        for &right_idx in right_row_indices {
                            matched_right_indices.insert(right_idx);
                            let right_row = &right_rows[right_idx];
                            let mut combined = left_row.clone();
                            // Add right columns, skipping USING columns
                            for (idx, val) in right_row.iter().enumerate() {
                                if !right_skip_indices.contains(&idx) {
                                    combined.push(val.clone());
                                }
                            }
                            result.push(combined);
                        }
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

    /// Extract equi-join keys from ON condition (e.g., left.id = right.user_id)
    /// Returns Some(JoinKeyInfo) if it's a simple equi-join, None otherwise
    fn extract_equi_join_keys(
        &self,
        expr: &crate::parser::ast::Expr,
        left_schema: &CombinedSchema,
        right_schema: &CombinedSchema,
    ) -> Option<JoinKeyInfo> {
        match expr {
            crate::parser::ast::Expr::BinaryOp { left, op, right } => {
                if *op != crate::parser::ast::BinaryOperator::Eq {
                    return None;
                }
                
                // Check if it's a simple column = column comparison
                let (left_col, right_col) = match (left.as_ref(), right.as_ref()) {
                    (
                        crate::parser::ast::Expr::Column(left_col),
                        crate::parser::ast::Expr::Column(right_col),
                    ) => (left_col, right_col),
                    (
                        crate::parser::ast::Expr::QualifiedColumn { table: left_table, column: left_col },
                        crate::parser::ast::Expr::QualifiedColumn { table: right_table, column: right_col },
                    ) => {
                        // Check if one is from left table and one from right
                        let left_in_left = left_schema.find_column(Some(left_table), left_col).is_ok();
                        let right_in_right = right_schema.find_column(Some(right_table), right_col).is_ok();
                        let left_in_right = right_schema.find_column(Some(left_table), left_col).is_ok();
                        let right_in_left = left_schema.find_column(Some(right_table), right_col).is_ok();
                        
                        if (left_in_left && right_in_right) || (left_in_right && right_in_left) {
                            // Determine which is which
                            if left_in_left && right_in_right {
                                (left_col, right_col)
                            } else {
                                (right_col, left_col)
                            }
                        } else {
                            return None;
                        }
                    }
                    (
                        crate::parser::ast::Expr::QualifiedColumn { table, column },
                        crate::parser::ast::Expr::Column(col),
                    ) => {
                        // Check if qualified is from left and unqualified from right, or vice versa
                        let qualified_in_left = left_schema.find_column(Some(table), column).is_ok();
                        let qualified_in_right = right_schema.find_column(Some(table), column).is_ok();
                        let unqualified_in_left = left_schema.find_column(None, col).is_ok();
                        let unqualified_in_right = right_schema.find_column(None, col).is_ok();
                        
                        if qualified_in_left && unqualified_in_right {
                            (column, col)
                        } else if qualified_in_right && unqualified_in_left {
                            (col, column)
                        } else {
                            return None;
                        }
                    }
                    (
                        crate::parser::ast::Expr::Column(col),
                        crate::parser::ast::Expr::QualifiedColumn { table, column },
                    ) => {
                        let qualified_in_left = left_schema.find_column(Some(table), column).is_ok();
                        let qualified_in_right = right_schema.find_column(Some(table), column).is_ok();
                        let unqualified_in_left = left_schema.find_column(None, col).is_ok();
                        let unqualified_in_right = right_schema.find_column(None, col).is_ok();
                        
                        if unqualified_in_left && qualified_in_right {
                            (col, column)
                        } else if unqualified_in_right && qualified_in_left {
                            (column, col)
                        } else {
                            return None;
                        }
                    }
                    _ => return None,
                };
                
                // Get column indices
                let left_idx = left_schema.find_column(None, left_col).ok()?;
                let right_idx = right_schema.find_column(None, right_col).ok()?;
                
                Some(JoinKeyInfo {
                    left_indices: vec![left_idx],
                    right_indices: vec![right_idx],
                })
            }
            crate::parser::ast::Expr::BinaryOp { left, op, right } if *op == crate::parser::ast::BinaryOperator::And => {
                // Handle AND of multiple equi-joins (multi-column join keys)
                let left_keys = self.extract_equi_join_keys(left, left_schema, right_schema)?;
                let right_keys = self.extract_equi_join_keys(right, left_schema, right_schema)?;
                
                Some(JoinKeyInfo {
                    left_indices: {
                        let mut v = left_keys.left_indices;
                        v.extend(right_keys.left_indices);
                        v
                    },
                    right_indices: {
                        let mut v = left_keys.right_indices;
                        v.extend(right_keys.right_indices);
                        v
                    },
                })
            }
            _ => None,
        }
    }

    /// Build hash table from rows using specified key indices
    fn build_hash_table(
        &self,
        rows: &[Vec<Value>],
        key_indices: &[usize],
    ) -> std::collections::HashMap<Vec<Value>, Vec<usize>> {
        use std::collections::HashMap;
        let mut hash_table: HashMap<Vec<Value>, Vec<usize>> = HashMap::new();
        
        for (row_idx, row) in rows.iter().enumerate() {
            let key: Vec<Value> = key_indices.iter().map(|&idx| row[idx].clone()).collect();
            hash_table.entry(key).or_insert_with(Vec::new).push(row_idx);
        }
        
        hash_table
    }

    /// Hash join for equi-joins
    /// Returns joined rows using hash join algorithm
    fn hash_join_equi(
        &self,
        left_rows: &[Vec<Value>],
        right_rows: &[Vec<Value>],
        key_info: &JoinKeyInfo,
    ) -> Vec<Vec<Value>> {
        // Choose build side (smaller table) and probe side (larger table)
        let (build_rows, probe_rows, build_indices, probe_indices) = if left_rows.len() <= right_rows.len() {
            (left_rows, right_rows, &key_info.left_indices, &key_info.right_indices)
        } else {
            (right_rows, left_rows, &key_info.right_indices, &key_info.left_indices)
        };
        
        // Build hash table on smaller table
        let hash_table = self.build_hash_table(build_rows, build_indices);
        
        // Probe with larger table
        let mut result = Vec::new();
        for probe_row in probe_rows.iter() {
            let probe_key: Vec<Value> = probe_indices.iter().map(|&idx| probe_row[idx].clone()).collect();
            
            if let Some(build_indices) = hash_table.get(&probe_key) {
                for &build_idx in build_indices {
                    let build_row = &build_rows[build_idx];
                    // Combine rows (order depends on which was build vs probe)
                    let combined_row = if left_rows.len() <= right_rows.len() {
                        [build_row.as_slice(), probe_row.as_slice()].concat()
                    } else {
                        [probe_row.as_slice(), build_row.as_slice()].concat()
                    };
                    result.push(combined_row);
                }
            }
        }
        
        result
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
