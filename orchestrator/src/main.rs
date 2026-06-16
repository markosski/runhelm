mod adapters;
mod api;
mod core;
mod ports;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::adapters::docker_executor::DockerExecutor;
use crate::adapters::memory_storage::MemoryStorage;
use crate::adapters::memory_workflow_queue::MemoryWorkflowQueue;
use crate::adapters::worker_pool::WorkerPool;
use crate::api::router;
use crate::core::function_service::FunctionService;
use crate::core::orchestrator::Orchestrator;
use crate::core::workflow::workflow_service::WorkflowService;
use crate::core::workspace_manager::{WorkspaceManager, WorkspaceManagerConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();
    info!("Starting RunHelm Orchestrator...");

    // Initialize dependencies (Adapters)
    let storage = Arc::new(MemoryStorage::new());
    let worker_pool = WorkerPool::new();
    let workspace_manager = Arc::new(WorkspaceManager::new(WorkspaceManagerConfig {
        root: PathBuf::from("/tmp/workspaces"),
        ttl: Duration::from_secs(300),
        vacuum_interval: Duration::from_secs(900),
    }));
    let executor = Arc::new(DockerExecutor::new(worker_pool.clone()));
    let workflow_queue = Arc::new(MemoryWorkflowQueue::new(workflow_queue_capacity()));

    // Initialize Orchestrator (Application Layer)
    let orchestrator = Arc::new(Orchestrator::new(
        storage.clone(),
        executor,
        workflow_queue,
        workspace_manager,
    ));
    let workflow_service = Arc::new(WorkflowService::new(storage.clone()));
    let function_service = Arc::new(FunctionService::new(storage.clone()));
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
    let public_app = router::create_public_router(
        orchestrator.clone(),
        workflow_service.clone(),
        function_service.clone(),
        worker_pool.clone(),
    );
    let worker_app = router::create_worker_router(
        orchestrator,
        workflow_service,
        function_service,
        worker_pool,
    );

    let public_addr = resolve_public_http_addr();
    let worker_addr = resolve_worker_http_addr();
    let public_listener = TcpListener::bind(&public_addr).await?;
    let worker_listener = TcpListener::bind(&worker_addr).await?;

    info!("Public API listening on {}", public_listener.local_addr()?);
    info!("Worker API listening on {}", worker_listener.local_addr()?);

    tokio::try_join!(
        axum::serve(public_listener, public_app),
        axum::serve(worker_listener, worker_app),
    )?;

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

fn resolve_public_http_addr() -> String {
    std::env::var("RUNHELM_PUBLIC_HTTP_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string())
}

fn resolve_worker_http_addr() -> String {
    std::env::var("RUNHELM_WORKER_HTTP_ADDR").unwrap_or_else(|_| "127.0.0.1:3001".to_string())
}
