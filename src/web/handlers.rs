use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

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

/// Execute a SQL query
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
        }));
    }

    let result = {
        let mut db = db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        db.execute_with_cache_info(query)
    };

    match result {
        Ok((query_result, from_cache)) => Ok(Json(ExecuteResponse {
            success: true,
            result: Some(query_result),
            error: None,
            from_cache: Some(from_cache),
        })),
        Err(e) => Ok(Json(ExecuteResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
            from_cache: None,
        })),
    }
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

