mod adapters;
mod api;
mod core;
mod ports;

use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::time::{self, Duration};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use crate::adapters::aws_storage::{AwsStorage, AwsStorageConfig};
use crate::adapters::memory_storage::MemoryStorage;
use crate::adapters::memory_workflow_queue::MemoryWorkflowQueue;
use crate::adapters::sql_storage::SqlStorage;
use crate::adapters::task_dispatcher::{self, TaskDispatcher};
use crate::adapters::worker_registry::WorkerRegistry;
use crate::api::router;
use crate::core::function::function_service::FunctionService;
use crate::core::orchestrator::Orchestrator;
use crate::core::workflow::workflow_service::WorkflowService;
use crate::ports::storage::StoragePort;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();
    info!("Starting RunHelm Orchestrator...");

    // Initialize dependencies (Adapters)
    let storage = create_storage().await?;
    let worker_registry = WorkerRegistry::new();
    let task_dispatcher = Arc::new(TaskDispatcher::new());
    let workflow_queue = Arc::new(MemoryWorkflowQueue::new(workflow_queue_capacity()));

    // Initialize Orchestrator (Application Layer)
    let orchestrator = Arc::new(Orchestrator::new(
        storage.clone(),
        task_dispatcher.clone(),
        workflow_queue,
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
            .run_workflow_queue(max_concurrent_workflows()),
    );

    // Setup API (Interface Layer)
    let public_app = router::create_public_router(
        orchestrator.clone(),
        workflow_service.clone(),
        function_service.clone(),
        worker_registry.clone(),
        task_dispatcher.clone(),
    );
    let worker_app = router::create_worker_router(
        orchestrator.clone(),
        workflow_service,
        function_service,
        worker_registry.clone(),
        task_dispatcher.clone(),
    );

    let public_addr = resolve_public_http_addr();
    let worker_addr = resolve_worker_http_addr();
    let public_listener = TcpListener::bind(&public_addr).await?;
    let worker_listener = TcpListener::bind(&worker_addr).await?;

    info!("Public API listening on {}", public_listener.local_addr()?);
    info!("Worker API listening on {}", worker_listener.local_addr()?);

    let _ = task_dispatcher::start_task_timeout_monitor(task_dispatcher.clone());
    let _ = start_pinned_host_loss_monitor(
        orchestrator.clone(),
        worker_registry.clone(),
        task_dispatcher.clone(),
    );

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

async fn create_storage() -> anyhow::Result<Arc<dyn StoragePort + Send + Sync>> {
    match std::env::var("RUNHELM_STORAGE")
        .unwrap_or_else(|_| "memory".to_string())
        .as_str()
    {
        "memory" => Ok(Arc::new(MemoryStorage::new())),
        "sql" => {
            let database_url = std::env::var("RUNHELM_DATABASE_URL").map_err(|_| {
                anyhow::anyhow!("RUNHELM_DATABASE_URL is required when RUNHELM_STORAGE=sql")
            })?;
            Ok(Arc::new(SqlStorage::connect(&database_url).await?))
        }
        "aws" => Ok(Arc::new(
            AwsStorage::connect(AwsStorageConfig::from_env()?).await?,
        )),
        value => anyhow::bail!("unsupported RUNHELM_STORAGE value {value}"),
    }
}

fn resolve_public_http_addr() -> String {
    std::env::var("RUNHELM_PUBLIC_HTTP_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string())
}

fn resolve_worker_http_addr() -> String {
    std::env::var("RUNHELM_WORKER_HTTP_ADDR").unwrap_or_else(|_| "127.0.0.1:3001".to_string())
}

fn start_pinned_host_loss_monitor(
    orchestrator: Arc<Orchestrator>,
    worker_registry: WorkerRegistry,
    task_dispatcher: Arc<TaskDispatcher>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = time::interval(Duration::from_millis(100));
        loop {
            ticker.tick().await;
            let lost_hosts = worker_registry.update_worker_liveness().await;
            if lost_hosts.is_empty() {
                continue;
            }
            task_dispatcher
                .cancel_pending_tasks_for_lost_hosts(&lost_hosts)
                .await;

            match orchestrator
                .fail_workflows_pinned_to_lost_hosts(&lost_hosts)
                .await
            {
                Ok(failed) => {
                    info!(failed, lost_hosts = ?lost_hosts, "Pinned host loss reconciliation complete");
                }
                Err(error) => {
                    error!(%error, lost_hosts = ?lost_hosts, "Pinned host loss reconciliation failed");
                }
            }
        }
    })
}
