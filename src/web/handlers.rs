use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::Database;

#[derive(Deserialize)]
pub struct ExecuteRequest {
    pub query: String,
}

#[derive(Serialize)]
pub struct ExecuteResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<crate::QueryResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_cache: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_time_ms: Option<f64>,
}

#[derive(Serialize)]
pub struct QueryProgressResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<crate::QueryProgressTracker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct QueryResultResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<crate::QueryResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_cache: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_time_ms: Option<f64>,
}

#[derive(Serialize)]
pub struct TableInfo {
    pub name: String,
    pub row_count: usize,
}

#[derive(Serialize)]
pub struct ListTablesResponse {
    pub success: bool,
    pub tables: Vec<TableInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Execute a SQL query (async with progress tracking for SELECT queries)
pub async fn execute_query(
    State(db): State<Arc<Mutex<Database>>>,
    Json(request): Json<ExecuteRequest>,
) -> Result<Json<ExecuteResponse>, StatusCode> {
    let query = request.query.trim();
    
    if query.is_empty() {
        return Ok(Json(ExecuteResponse {
            success: false,
            result: None,
            error: Some("Query cannot be empty".to_string()),
            from_cache: None,
            query_id: None,
            execution_time_ms: None,
        }));
    }

    // Check if this is a SELECT query - if so, use async execution with progress
    let is_select = query.trim_start().to_uppercase().starts_with("SELECT");
    
    if is_select {
        // Generate query ID
        let query_id = Uuid::new_v4().to_string();
        
        // Start async execution
        if let Err(e) = Database::execute_async_internal(db.clone(), query_id.clone(), query.to_string()) {
            return Ok(Json(ExecuteResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
                from_cache: None,
                query_id: None,
                execution_time_ms: None,
            }));
        }
        
        // Return query ID immediately (execution time will be included in result response)
        Ok(Json(ExecuteResponse {
            success: true,
            result: None,
            error: None,
            from_cache: None,
            query_id: Some(query_id),
            execution_time_ms: None,
        }))
    } else {
        // For non-SELECT queries, execute synchronously
        let result = {
            let mut db = db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            db.execute_with_cache_info(query)
        };

        match result {
            Ok((query_result, from_cache, execution_time_ms)) => Ok(Json(ExecuteResponse {
                success: true,
                result: Some(query_result),
                error: None,
                from_cache: Some(from_cache),
                query_id: None,
                execution_time_ms: Some(execution_time_ms),
            })),
            Err(e) => Ok(Json(ExecuteResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
                from_cache: None,
                query_id: None,
                execution_time_ms: None,
            })),
        }
    }
}

/// Get query progress
pub async fn get_query_progress(
    State(db): State<Arc<Mutex<Database>>>,
    Path(query_id): Path<String>,
) -> Result<Json<QueryProgressResponse>, StatusCode> {
    let progress = {
        let db = db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        db.get_query_progress(&query_id)
    };

    match progress {
        Some(mut tracker) => {
            // Update elapsed time
            tracker.update_elapsed();
            Ok(Json(QueryProgressResponse {
                success: true,
                progress: Some(tracker),
                error: None,
            }))
        }
        None => Ok(Json(QueryProgressResponse {
            success: false,
            progress: None,
            error: Some("Query not found".to_string()),
        })),
    }
}

/// Get query result
pub async fn get_query_result(
    State(db): State<Arc<Mutex<Database>>>,
    Path(query_id): Path<String>,
) -> Result<Json<QueryResultResponse>, StatusCode> {
    let result_storage = {
        let db = db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        db.get_query_result(&query_id)
    };

    match result_storage {
        Some(storage) => Ok(Json(QueryResultResponse {
            success: true,
            result: storage.result,
            from_cache: Some(storage.from_cache),
            error: None,
            execution_time_ms: Some(storage.execution_time_ms),
        })),
        None => Ok(Json(QueryResultResponse {
            success: false,
            result: None,
            from_cache: None,
            error: Some("Query result not found or query still running".to_string()),
            execution_time_ms: None,
        })),
    }
}

/// Cancel a query
pub async fn cancel_query(
    State(db): State<Arc<Mutex<Database>>>,
    Path(query_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let cancelled = {
        let db = db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        db.cancel_query(&query_id)
    };

    Ok(Json(serde_json::json!({
        "success": cancelled,
        "message": if cancelled { "Query cancelled" } else { "Query not found" }
    })))
}

/// List all tables in the database
pub async fn list_tables(
    State(db): State<Arc<Mutex<Database>>>,
) -> Result<Json<ListTablesResponse>, StatusCode> {
    let tables = {
        let db = db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let table_names = db.list_tables();
        let mut table_infos = Vec::new();
        
        for table_name in table_names {
            let row_count = db.get_table_row_count(&table_name).unwrap_or(0);
            table_infos.push(TableInfo {
                name: table_name,
                row_count,
            });
        }
        
        table_infos
    };

    Ok(Json(ListTablesResponse {
        success: true,
        tables,
        error: None,
    }))
}

#[derive(Serialize)]
pub struct TableSchemaResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<crate::TableSchema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Get table schema information
pub async fn get_table_schema(
    State(db): State<Arc<Mutex<Database>>>,
    Path(table_name): Path<String>,
) -> Result<Json<TableSchemaResponse>, StatusCode> {
    let schema = {
        let db = db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        db.get_table_schema(&table_name)
    };

    match schema {
        Ok(schema) => Ok(Json(TableSchemaResponse {
            success: true,
            schema: Some(schema),
            error: None,
        })),
        Err(e) => Ok(Json(TableSchemaResponse {
            success: false,
            schema: None,
            error: Some(e.to_string()),
        })),
    }
}

