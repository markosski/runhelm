mod adapters;
mod api;
mod core;
mod ports;

use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::adapters::docker_executor::DockerExecutor;
use crate::adapters::ipc::{WorkerPool, run_ipc_server, socket_path_from_env};
use crate::adapters::memory_storage::MemoryStorage;
use crate::api::router;
use crate::core::orchestrator::Orchestrator;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();
    info!("Starting RunHelm Orchestrator...");

    // Initialize dependencies (Adapters)
    let storage = Arc::new(MemoryStorage::new());
    let worker_pool = WorkerPool::new();
    let executor = Arc::new(DockerExecutor::new(worker_pool.clone()));

    // Initialize Orchestrator (Application Layer)
    let orchestrator = Arc::new(Orchestrator::new(storage, executor));
    let recovered = orchestrator.synchronize_startup_tasks().await?;
    info!(recovered, "Startup task synchronization complete");

    // Setup router
    let app = router::create_router(orchestrator);

    let socket_path = socket_path_from_env();
    tokio::spawn(async move {
        if let Err(error) = run_ipc_server(socket_path, worker_pool).await {
            tracing::error!(%error, "IPC server stopped");
        }
    });

    // Start server
    let listener = TcpListener::bind("0.0.0.0:3000").await?;
    info!("Listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    Ok(())
}
