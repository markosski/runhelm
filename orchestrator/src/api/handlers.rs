use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use std::time::Duration;
use tracing::info;

use crate::adapters::worker_pool::{
    TaskResult, WorkerExecutionResult, WorkerRegistration, WorkerResponse,
};
use crate::core::models::FunctionDef;
use crate::core::workflow::models::{WorkflowDef, WorkflowStatus};
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
        .workflow_service
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

pub async fn create_function_def(
    State(state): State<AppState>,
    Json(function_def): Json<FunctionDef>,
) -> Result<Json<Value>, StatusCode> {
    let function_def_id = function_def.id.clone();

    state
        .function_service
        .create_function_def(function_def)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    info!(
        id = %function_def_id,
        "Registered function definition"
    );

    Ok(Json(json!({
        "status": "created",
        "id": function_def_id
    })))
}

pub async fn delete_function_def(
    State(state): State<AppState>,
    Path(function_def_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    match state
        .function_service
        .delete_function_def(&function_def_id)
        .await
    {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn trigger_workflow_instance(
    State(state): State<AppState>,
    Path(workflow_def_id): Path<String>,
    Json(_payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let instance_id = state
        .workflow_service
        .create_workflow_instance_for_def(&workflow_def_id)
        .await
        .map_err(|error| {
            if error.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

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

    match state.workflow_service.list_workflows(status).await {
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
        .workflow_service
        .get_task_result(&workflow_instance_id, &task_id)
        .await
    {
        Ok(result) => Ok(Json(serde_json::to_value(result).unwrap())),
        Err(error) if error.to_string().contains("not found") => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn list_task_results(
    State(state): State<AppState>,
    Path(workflow_instance_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state
        .workflow_service
        .list_task_results(&workflow_instance_id)
        .await
    {
        Ok(tasks) => Ok(Json(json!({
            "workflow_instance_id": workflow_instance_id,
            "tasks": tasks,
        }))),
        Err(error) if error.to_string().contains("not found") => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_task_result_generation(
    State(state): State<AppState>,
    Path((workflow_instance_id, task_id, generation)): Path<(String, String, u32)>,
) -> Result<Json<Value>, StatusCode> {
    match state
        .workflow_service
        .get_task_result_for_generation(&workflow_instance_id, &task_id, Some(generation))
        .await
    {
        Ok(result) => Ok(Json(serde_json::to_value(result).unwrap())),
        Err(error) if error.to_string().contains("not found") => Err(StatusCode::NOT_FOUND),
        Err(error) if error.to_string().contains("generation must be positive") => {
            Err(StatusCode::BAD_REQUEST)
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[derive(Deserialize)]
pub struct WorkerClaimRequest {
    worker_id: String,
}

pub async fn register_worker(
    State(state): State<AppState>,
    Json(registration): Json<WorkerRegistration>,
) -> Result<Json<Value>, StatusCode> {
    let worker_id = registration.worker_id.clone();
    state.worker_pool.register_worker(registration).await;

    Ok(Json(
        serde_json::to_value(WorkerResponse::RegistrationAck { worker_id }).unwrap(),
    ))
}

pub async fn claim_worker_task(
    State(state): State<AppState>,
    Json(payload): Json<WorkerClaimRequest>,
) -> Result<Json<Value>, StatusCode> {
    match state
        .worker_pool
        .claim_task(&payload.worker_id, Duration::from_secs(30))
        .await
    {
        Ok(Some(dispatch)) => Ok(Json(
            serde_json::to_value(WorkerResponse::TaskDispatch(dispatch)).unwrap(),
        )),
        Ok(None) => Ok(Json(serde_json::to_value(WorkerResponse::NoTask).unwrap())),
        Err(error) if error.to_string().contains("not registered") => Err(StatusCode::NOT_FOUND),
        Err(error) => {
            tracing::error!(worker_id = %payload.worker_id, %error, "worker failed to claim task");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn complete_worker_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(result): Json<WorkerExecutionResult>,
) -> Result<Json<Value>, StatusCode> {
    let worker_id = state.worker_pool.worker_for_task(&task_id).await;

    if let Some(worker_id) = worker_id {
        state
            .worker_pool
            .complete_task_result(&worker_id, TaskResult { task_id, result })
            .await
            .map_err(|error| {
                tracing::error!(%worker_id, %error, "worker failed to complete task");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    } else {
        tracing::warn!(%task_id, "acknowledging late or untracked worker task result");
    }

    Ok(Json(json!({ "status": "accepted" })))
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
