use axum::{
    Router,
    routing::{delete, get, post},
};
use std::sync::Arc;

use super::handlers;

use crate::core::orchestrator::Orchestrator;

// AppState holds the injected dependencies
#[derive(Clone)]
pub struct AppState {
    pub orchestrator: Arc<Orchestrator>,
}

pub fn create_router(orchestrator: Arc<Orchestrator>) -> Router {
    let state = AppState { orchestrator };

    Router::new()
        .route("/health", get(handlers::health_check))
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
            "/queue",
            get(handlers::get_queue).delete(handlers::purge_queue),
        )
        .route("/queue/{id}", delete(handlers::delete_queue_item))
        .route("/workflows", get(handlers::list_workflows))
        .route("/workflows/{id}", get(handlers::get_workflow_instance))
        .route(
            "/workflows/{workflow_instance_id}/tasks/{task_id}",
            get(handlers::get_task_result),
        )
        .fallback(handlers::not_found)
        .with_state(state)
}
