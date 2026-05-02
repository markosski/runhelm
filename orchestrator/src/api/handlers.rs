use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};

use super::router::AppState;

pub async fn health_check() -> &'static str {
    "OK"
}

pub async fn create_workflow(
    State(_state): State<AppState>,
    Json(_payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // TODO: Parse workflow, save via state.storage, return ID
    Ok(Json(json!({ "status": "created", "id": "placeholder-id" })))
}

pub async fn get_workflow(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    // TODO: Fetch workflow from state.storage
    Ok(Json(json!({ "id": id, "status": "pending" })))
}
