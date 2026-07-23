use axum::{
    Router,
    routing::{delete, get, post},
};
use std::sync::Arc;

use super::handlers;

use crate::adapters::task_dispatcher::TaskDispatcher;
use crate::adapters::worker_registry::WorkerRegistry;
use crate::core::function::function_service::FunctionService;
use crate::core::namespace::NamespaceResolverPort;
use crate::core::orchestrator::Orchestrator;
use crate::core::workflow::workflow_service::WorkflowService;

#[derive(Clone)]
pub struct PublicAppState {
    pub orchestrator: Arc<Orchestrator>,
    pub workflow_service: Arc<WorkflowService>,
    pub function_service: Arc<FunctionService>,
    pub worker_registry: WorkerRegistry,
    pub namespace_resolver: Arc<dyn NamespaceResolverPort + Send + Sync>,
}

#[derive(Clone)]
pub struct WorkerAppState {
    pub worker_registry: WorkerRegistry,
    pub task_dispatcher: Arc<TaskDispatcher>,
}

pub fn create_public_router(
    orchestrator: Arc<Orchestrator>,
    workflow_service: Arc<WorkflowService>,
    function_service: Arc<FunctionService>,
    worker_registry: WorkerRegistry,
    namespace_resolver: Arc<dyn NamespaceResolverPort + Send + Sync>,
) -> Router {
    let state = PublicAppState {
        orchestrator,
        workflow_service,
        function_service,
        worker_registry,
        namespace_resolver,
    };

    Router::new()
        .route("/health", get(handlers::health_check))
        .route("/function-def", post(handlers::create_function_def))
        .route(
            "/function-def/{def_id}",
            delete(handlers::delete_function_def),
        )
        .route(
            "/workflow-def",
            get(handlers::list_workflow_defs).post(handlers::create_workflow_def),
        )
        .route(
            "/workflow-def/{def_id}/tasks/{task_id}",
            post(handlers::invoke_workflow_task_isolated),
        )
        .route(
            "/workflow-def/{def_id}",
            get(handlers::get_workflow_def).post(handlers::trigger_workflow_instance),
        )
        .route(
            "/workflow-queue",
            get(handlers::get_queue).delete(handlers::purge_queue),
        )
        .route("/workflow-queue/{id}", delete(handlers::delete_queue_item))
        .route("/workflows", get(handlers::list_workflows))
        .route("/workflows/pause", post(handlers::pause_active_workflows))
        .route("/workflows/resume", post(handlers::resume_paused_workflows))
        .route("/workflows/{id}/events", get(handlers::get_workflow_events))
        .route("/workflows/{id}/pause", post(handlers::pause_workflow))
        .route("/workflows/{id}/resume", post(handlers::resume_workflow))
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
            "/workflows/{workflow_instance_id}/tasks/{task_id}/retry",
            post(handlers::retry_task),
        )
        .route(
            "/workflows/{workflow_instance_id}/tasks/{task_id}",
            get(handlers::get_task_result),
        )
        .fallback(handlers::not_found)
        .with_state(state)
}

pub fn create_worker_router(
    worker_registry: WorkerRegistry,
    task_dispatcher: Arc<TaskDispatcher>,
) -> Router {
    let state = WorkerAppState {
        worker_registry,
        task_dispatcher,
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
    use crate::adapters::fake_task_dispatcher::FakeTaskDispatcher;
    use crate::adapters::memory_storage::MemoryStorage;
    use crate::adapters::memory_workflow_queue::MemoryWorkflowQueue;
    use crate::core::namespace::NamespaceResolver;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[test]
    fn public_router_accepts_task_action_route_shapes() {
        let storage = Arc::new(MemoryStorage::new());
        let orchestrator = Arc::new(Orchestrator::new(
            storage.clone(),
            Arc::new(FakeTaskDispatcher::new()),
            Arc::new(MemoryWorkflowQueue::new(10)),
        ));

        let _router = create_public_router(
            orchestrator,
            Arc::new(WorkflowService::new(storage.clone())),
            Arc::new(FunctionService::new(storage.clone())),
            WorkerRegistry::new(),
            Arc::new(NamespaceResolver::new(storage)),
        );
    }

    #[tokio::test]
    async fn public_health_check_does_not_require_namespace_context() {
        let storage = Arc::new(MemoryStorage::new());
        let orchestrator = Arc::new(Orchestrator::new(
            storage.clone(),
            Arc::new(FakeTaskDispatcher::new()),
            Arc::new(MemoryWorkflowQueue::new(10)),
        ));
        let router = create_public_router(
            orchestrator,
            Arc::new(WorkflowService::new(storage.clone())),
            Arc::new(FunctionService::new(storage.clone())),
            WorkerRegistry::new(),
            Arc::new(NamespaceResolver::new(storage)),
        );

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn public_resource_route_rejects_missing_namespace_context() {
        let storage = Arc::new(MemoryStorage::new());
        let orchestrator = Arc::new(Orchestrator::new(
            storage.clone(),
            Arc::new(FakeTaskDispatcher::new()),
            Arc::new(MemoryWorkflowQueue::new(10)),
        ));
        let router = create_public_router(
            orchestrator,
            Arc::new(WorkflowService::new(storage.clone())),
            Arc::new(FunctionService::new(storage.clone())),
            WorkerRegistry::new(),
            Arc::new(NamespaceResolver::new(storage)),
        );

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/workflow-def")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
    }
}
