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
    // TODO: Parse workflow, save via state.orchestrator, return ID
    Ok(Json(json!({ "status": "created", "id": "placeholder-id" })))
}

pub async fn get_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state.orchestrator.get_workflow_status(&id).await {
        Ok(Some(report)) => Ok(Json(serde_json::to_value(report).unwrap())),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
