use crate::Database;
use axum::{
    routing::{delete, get, post},
    Router,
};
use std::sync::{Arc, Mutex};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use super::handlers;

/// Start the web server
pub async fn start_server(db: Arc<Mutex<Database>>) -> anyhow::Result<()> {
    // Start background flush task for automatic flushing
    start_background_flush_task(db.clone());
    
    // Get the absolute path to the web directory
    let web_dir = std::env::current_dir()?
        .join("web");
    
    println!("Serving static files from: {:?}", web_dir);
    
    // API routes must come before static file service
    let api_routes = Router::new()
        .route("/api/execute", post(handlers::execute_query))
        .route("/api/tables", get(handlers::list_tables))
        .route("/api/table/:name", get(handlers::get_table_schema))
        .route("/api/query/:query_id/progress", get(handlers::get_query_progress))
        .route("/api/query/:query_id/result", get(handlers::get_query_result))
        .route("/api/query/:query_id", delete(handlers::cancel_query));
    
    let app = Router::new()
        .merge(api_routes)
        .nest_service("/", ServeDir::new(web_dir))
        .layer(CorsLayer::permissive())
        .with_state(db);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    println!("Server running on http://localhost:8080");
    
    // Try to open browser
    if let Err(e) = open_browser() {
        eprintln!("Failed to open browser: {}", e);
    }
    
    axum::serve(listener, app).await?;
    Ok(())
}

/// Start background task for automatic flushing
fn start_background_flush_task(db: Arc<Mutex<Database>>) {
    use tokio::time::{interval, Duration};
    
    // Spawn background task that runs every 3 seconds
    tokio::spawn(async move {
        let mut flush_interval = interval(Duration::from_secs(3));
        let mut safety_flush_interval = interval(Duration::from_secs(8)); // Safety net every 8 seconds
        
        loop {
            tokio::select! {
                _ = flush_interval.tick() => {
                    // Regular flush check every 3 seconds
                    if let Ok(mut db_guard) = db.lock() {
                        if let Err(e) = db_guard.storage.flush_all_pending_buffers() {
                            eprintln!("Warning: Background flush task error: {}", e);
                        }
                    }
                }
                _ = safety_flush_interval.tick() => {
                    // Safety flush every 8 seconds - flush ALL tables with any pending data
                    if let Ok(mut db_guard) = db.lock() {
                        // Force flush all tables with pending buffers (safety net)
                        let tables = db_guard.storage.get_tables_with_pending_buffers();
                        
                        for table_name in tables {
                            // Check if table still has pending data
                            let has_pending = db_guard.storage.has_pending_buffer_data(&table_name)
                                || db_guard.storage.has_pending_data_files(&table_name);
                            
                            if has_pending {
                                if let Err(e) = db_guard.storage.flush_table_iceberg_writes(&table_name) {
                                    eprintln!("Warning: Safety flush failed for table {}: {}", table_name, e);
                                }
                            }
                        }
                    }
                }
            }
        }
    });
}

fn open_browser() -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("http://localhost:8080")
            .spawn()?;
    }
    
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg("http://localhost:8080")
            .spawn()?;
    }
    
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "http://localhost:8080"])
            .spawn()?;
    }
    
    Ok(())
}
