use std::sync::{Arc, Mutex};
use vtdb::Database;
use vtdb::web::start_server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create a shared database instance
    let db = Arc::new(Mutex::new(Database::new()));
    
    // Start the web server
    start_server(db).await?;
    
    Ok(())
}





