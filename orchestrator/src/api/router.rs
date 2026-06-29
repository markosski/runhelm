use axum::{
    Router,
    routing::{delete, get, post},
};
use std::sync::Arc;

use super::handlers;

use crate::core::function_service::FunctionService;
use crate::core::orchestrator::Orchestrator;
use crate::{adapters::worker_pool::WorkerPool, core::workflow::workflow_service::WorkflowService};

// AppState holds the injected dependencies
#[derive(Clone)]
pub struct AppState {
    pub orchestrator: Arc<Orchestrator>,
    pub workflow_service: Arc<WorkflowService>,
    pub function_service: Arc<FunctionService>,
    pub worker_pool: WorkerPool,
}

pub fn create_public_router(
    orchestrator: Arc<Orchestrator>,
    workflow_service: Arc<WorkflowService>,
    function_service: Arc<FunctionService>,
    worker_pool: WorkerPool,
) -> Router {
    let state = AppState {
        orchestrator,
        workflow_service,
        function_service,
        worker_pool,
    };

    Router::new()
        .route("/health", get(handlers::health_check))
        .route("/function-def", post(handlers::create_function_def))
        .route(
            "/function-def/{def_id}",
            delete(handlers::delete_function_def),
        )
        .route("/workflow-def", post(handlers::create_workflow_def))
        .route(
            "/workflow-def/{def_id}/tasks/{task_id}",
            post(handlers::invoke_workflow_task_isolated),
        )
        .route(
            "/workflow-def/{def_id}",
            post(handlers::trigger_workflow_instance),
        )
        .route(
            "/workflow-queue",
            get(handlers::get_queue).delete(handlers::purge_queue),
        )
        .route("/workflow-queue/{id}", delete(handlers::delete_queue_item))
        .route("/workflows", get(handlers::list_workflows))
        .route("/workflows/{id}/events", get(handlers::get_workflow_events))
        .route("/workflows/{id}", get(handlers::get_workflow_instance))
        .route(
            "/workflows/{workflow_instance_id}/tasks",
            get(handlers::list_task_results),
        )
        .route(
            "/workflows/{workflow_instance_id}/tasks/{task_id}/{generation}",
            get(handlers::get_task_result_generation),
        )
        .route(
            "/workflows/{workflow_instance_id}/tasks/{task_id}/human-input",
            post(handlers::submit_human_input),
        )
        .route(
            "/workflows/{workflow_instance_id}/tasks/{task_id}",
            get(handlers::get_task_result),
        )
        .fallback(handlers::not_found)
        .with_state(state)
}

pub fn create_worker_router(
    orchestrator: Arc<Orchestrator>,
    workflow_service: Arc<WorkflowService>,
    function_service: Arc<FunctionService>,
    worker_pool: WorkerPool,
) -> Router {
    let state = AppState {
        orchestrator,
        workflow_service,
        function_service,
        worker_pool,
    };

    Router::new()
        .route("/health", get(handlers::health_check))
        .route("/workers/register", post(handlers::register_worker))
        .route("/workers/heartbeat", post(handlers::heartbeat_worker))
        .route("/workers/tasks/claim", post(handlers::claim_worker_task))
        .route(
            "/workers/tasks/{task_id}/result",
            post(handlers::complete_worker_task),
        )
        .fallback(handlers::not_found)
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake_executor::FakeExecutor;
    use crate::adapters::memory_storage::MemoryStorage;
    use crate::adapters::memory_workflow_queue::MemoryWorkflowQueue;

    #[test]
    fn public_router_accepts_human_input_route_shape() {
        let storage = Arc::new(MemoryStorage::new());
        let orchestrator = Arc::new(Orchestrator::new(
            storage.clone(),
            Arc::new(FakeExecutor::new()),
            Arc::new(MemoryWorkflowQueue::new(10)),
        ));

        let _router = create_public_router(
            orchestrator,
            Arc::new(WorkflowService::new(storage.clone())),
            Arc::new(FunctionService::new(storage)),
            WorkerPool::new(),
        );
    }
}
