use axum::{routing::get, routing::post, Router};
use std::sync::Arc;

use super::handlers;
use crate::ports::storage::StoragePort;

// AppState holds the injected dependencies
#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<dyn StoragePort + Send + Sync>,
}

pub fn create_router(storage: Arc<dyn StoragePort + Send + Sync>) -> Router {
    let state = AppState { storage };

    Router::new()
        .route("/health", get(handlers::health_check))
        .route("/workflows", post(handlers::create_workflow))
        .route("/workflows/:id", get(handlers::get_workflow))
        .with_state(state)
}
