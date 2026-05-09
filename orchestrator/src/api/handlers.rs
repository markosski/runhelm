use axum::{
    Json,
    extract::{Path, State},
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

    let orchestrator = state.orchestrator.clone();
    let run_instance_id = instance_id.clone();
    tokio::spawn(async move {
        if let Err(error) = orchestrator.run_workflow(run_instance_id.clone()).await {
            tracing::error!(
                workflow_instance_id = %run_instance_id,
                %error,
                "workflow execution failed"
            );
        }
    });

    info!("Created workflow instance with ID: {}", instance_id);

    Ok(Json(json!({
        "status": "created",
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
