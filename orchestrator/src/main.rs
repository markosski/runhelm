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
use crate::adapters::memory_workflow_queue::MemoryWorkflowQueue;
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
    let workflow_queue = Arc::new(MemoryWorkflowQueue::new(workflow_queue_capacity()));

    // Initialize Orchestrator (Application Layer)
    let orchestrator = Arc::new(Orchestrator::new(storage, executor, workflow_queue));
    let recovered = orchestrator.synchronize_startup_tasks().await?;
    info!(recovered, "Startup task synchronization complete");
    let requeued = orchestrator.enqueue_active_workflow_instances().await?;
    info!(requeued, "Active workflow requeue complete");
    tokio::spawn(
        orchestrator
            .clone()
            .run_scheduler(max_concurrent_workflows()),
    );

    // Setup API (Interface Layer)
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

fn max_concurrent_workflows() -> usize {
    std::env::var("RUNHELM_MAX_CONCURRENT_WORKFLOWS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1)
}

fn workflow_queue_capacity() -> usize {
    std::env::var("RUNHELM_WORKFLOW_QUEUE_CAPACITY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1024)
}
