use crate::Database;
use axum::{
    routing::{get, post},
    Router,
};
use std::sync::{Arc, Mutex};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use super::handlers;

/// Start the web server
pub async fn start_server(db: Arc<Mutex<Database>>) -> anyhow::Result<()> {
    // Get the absolute path to the web directory
    let web_dir = std::env::current_dir()?
        .join("web");
    
    println!("Serving static files from: {:?}", web_dir);
    
    let app = Router::new()
        .route("/api/execute", post(handlers::execute_query))
        .route("/api/tables", get(handlers::list_tables))
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
