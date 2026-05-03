use axum::{routing::get, routing::post, Router};
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
        .route("/workflows", post(handlers::create_workflow))
        .route("/workflows/:id", get(handlers::get_workflow))
        .with_state(state)
}
