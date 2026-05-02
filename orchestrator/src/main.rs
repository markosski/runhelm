mod adapters;
mod api;
mod core;
mod ports;

use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;

use crate::adapters::memory_storage::MemoryStorage;
use crate::api::router;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    info!("Starting RunHelm Orchestrator...");

    // Initialize dependencies (Adapters)
    let storage = Arc::new(MemoryStorage::new());

    // Setup router
    let app = router::create_router(storage);

    // Start server
    let listener = TcpListener::bind("0.0.0.0:3000").await?;
    info!("Listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    Ok(())
}
