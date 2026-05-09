use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

use crate::core::models::{WorkflowDef, WorkflowInstance, WorkflowStatus};
use crate::ports::executor::ExecutionResult;
use serde::Deserialize;
use serde_json::{Value, json};

use super::router::AppState;

pub async fn health_check() -> &'static str {
    "OK"
}

pub async fn not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}

pub async fn create_workflow_def(
    State(state): State<AppState>,
    Json(workflow_def): Json<WorkflowDef>,
) -> Result<Json<Value>, StatusCode> {
    let workflow_def_id = workflow_def.id.clone();

    state
        .orchestrator
        .create_workflow_def(workflow_def)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    info!(
        "Registered workflow definition with ID: {}",
        workflow_def_id
    );

    Ok(Json(json!({
        "status": "created",
        "id": workflow_def_id
    })))
}

pub async fn trigger_workflow_instance(
    State(state): State<AppState>,
    Path(workflow_def_id): Path<String>,
    Json(_payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let Some(_) = state
        .orchestrator
        .get_workflow_def(&workflow_def_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    else {
        return Err(StatusCode::NOT_FOUND);
    };

    let instance_id = create_instance_id(&workflow_def_id)?;
    let instance = WorkflowInstance {
        id: instance_id.clone(),
        workflow_def_id,
        status: WorkflowStatus::Pending,
        tasks: HashMap::new(),
    };

    state
        .orchestrator
        .create_workflow_instance(instance)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    state
        .orchestrator
        .enqueue_workflow_instance(instance_id.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    info!("Created queued workflow instance with ID: {}", instance_id);

    Ok(Json(json!({
        "status": "queued",
        "id": instance_id
    })))
}

#[derive(Deserialize)]
pub struct InvokeTaskRequest {
    inputs: Vec<Value>,
}

pub async fn invoke_workflow_task_isolated(
    State(state): State<AppState>,
    Path((workflow_def_id, task_id)): Path<(String, String)>,
    Json(payload): Json<InvokeTaskRequest>,
) -> Result<Json<Value>, StatusCode> {
    match state
        .orchestrator
        .execute_workflow_task_isolated(&workflow_def_id, &task_id, &payload.inputs)
        .await
    {
        Ok(Some(result)) => Ok(Json(execution_result_to_value(result))),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(error) => {
            tracing::error!(%workflow_def_id, %task_id, %error, "isolated task execution failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_workflow_instance(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state.orchestrator.get_workflow_status(&id).await {
        Ok(Some(report)) => Ok(Json(serde_json::to_value(report).unwrap())),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[derive(Deserialize)]
pub struct WorkflowListQuery {
    status: Option<String>,
}

pub async fn list_workflows(
    State(state): State<AppState>,
    Query(query): Query<WorkflowListQuery>,
) -> Result<Json<Value>, StatusCode> {
    let status = query
        .status
        .as_deref()
        .map(parse_workflow_status)
        .transpose()?;

    match state.orchestrator.list_workflows(status).await {
        Ok(workflows) => Ok(Json(serde_json::to_value(workflows).unwrap())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_queue(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    match state.orchestrator.get_queue_status().await {
        Ok(status) => Ok(Json(serde_json::to_value(status).unwrap())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn delete_queue_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    match state
        .orchestrator
        .remove_queued_workflow_instance(&id)
        .await
    {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn purge_queue(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    match state.orchestrator.purge_queued_workflow_instances().await {
        Ok(purged) => Ok(Json(json!({
            "status": "purged",
            "purged": purged,
        }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_task_result(
    State(state): State<AppState>,
    Path((workflow_instance_id, task_id)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state
        .orchestrator
        .get_task_result(&workflow_instance_id, &task_id)
        .await
    {
        Ok(result) => Ok(Json(serde_json::to_value(result).unwrap())),
        Err(error) if error.to_string().contains("not found") => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

fn execution_result_to_value(result: ExecutionResult) -> Value {
    match result {
        ExecutionResult::Success(output) => json!({
            "status": "success",
            "output": output
        }),
        ExecutionResult::InputNeeded(description) => json!({
            "status": "input_needed",
            "description": description
        }),
        ExecutionResult::Failure(error_message) => json!({
            "status": "failure",
            "error_message": error_message
        }),
    }
}

fn create_instance_id(workflow_def_id: &str) -> Result<String, StatusCode> {
    let timestamp_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .as_nanos();
    Ok(format!("{workflow_def_id}-{timestamp_nanos}"))
}

fn parse_workflow_status(status: &str) -> Result<WorkflowStatus, StatusCode> {
    match status.to_ascii_lowercase().as_str() {
        "pending" => Ok(WorkflowStatus::Pending),
        "running" => Ok(WorkflowStatus::Running),
        "paused" => Ok(WorkflowStatus::Paused),
        "input_needed" | "input-needed" => Ok(WorkflowStatus::InputNeeded),
        "completed" => Ok(WorkflowStatus::Completed),
        "failed" => Ok(WorkflowStatus::Failed),
        _ => Err(StatusCode::BAD_REQUEST),
    }
}
