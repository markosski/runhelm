use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::info;

use crate::adapters::worker_pool::{
    TaskResult, WorkerExecutionResult, WorkerRegistration, WorkerResponse,
};
use crate::core::models::{FunctionDef, WorkflowDef, WorkflowInstance, WorkflowStatus};
use crate::ports::auth::AuthContext;
use crate::ports::executor::ExecutionResult;
use serde::Deserialize;
use serde_json::{Value, json};

use super::router::AppState;

async fn authenticate_request(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AuthContext, StatusCode> {
    let Some(header) = headers.get(axum::http::header::AUTHORIZATION) else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let value = header.to_str().map_err(|_| StatusCode::UNAUTHORIZED)?;
    let Some((scheme, token)) = value.split_once(' ') else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    if !scheme.eq_ignore_ascii_case("Bearer") || token.trim().is_empty() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    state
        .auth
        .authenticate_api_token(token.trim())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)
}

pub async fn health_check() -> &'static str {
    "OK"
}

pub async fn not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}

pub async fn create_workflow_def(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(workflow_def): Json<WorkflowDef>,
) -> Result<Json<Value>, StatusCode> {
    let auth = authenticate_request(&state, &headers).await?;
    let workflow_def_id = workflow_def.id.clone();

    state
        .orchestrator
        .create_workflow_def(&auth.namespace_id, workflow_def)
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
    headers: HeaderMap,
    Json(function_def): Json<FunctionDef>,
) -> Result<Json<Value>, StatusCode> {
    let auth = authenticate_request(&state, &headers).await?;
    let function_def_id = function_def.id.clone();

    state
        .orchestrator
        .create_function_def(&auth.namespace_id, function_def)
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
    headers: HeaderMap,
    Path(function_def_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let auth = authenticate_request(&state, &headers).await?;
    match state
        .orchestrator
        .delete_function_def(&auth.namespace_id, &function_def_id)
        .await
    {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn trigger_workflow_instance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workflow_def_id): Path<String>,
    Json(_payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let auth = authenticate_request(&state, &headers).await?;
    let Some(_) = state
        .orchestrator
        .get_workflow_def(&auth.namespace_id, &workflow_def_id)
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
        .create_workflow_instance(&auth.namespace_id, instance)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    state
        .orchestrator
        .enqueue_workflow_instance(auth.namespace_id, instance_id.clone())
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
    headers: HeaderMap,
    Path((workflow_def_id, task_id)): Path<(String, String)>,
    Json(payload): Json<InvokeTaskRequest>,
) -> Result<Json<Value>, StatusCode> {
    let auth = authenticate_request(&state, &headers).await?;
    match state
        .orchestrator
        .execute_workflow_task_isolated(
            &auth.namespace_id,
            &workflow_def_id,
            &task_id,
            &payload.inputs,
        )
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
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let auth = authenticate_request(&state, &headers).await?;
    match state
        .orchestrator
        .get_workflow_status(&auth.namespace_id, &id)
        .await
    {
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
    headers: HeaderMap,
    Query(query): Query<WorkflowListQuery>,
) -> Result<Json<Value>, StatusCode> {
    let auth = authenticate_request(&state, &headers).await?;
    let status = query
        .status
        .as_deref()
        .map(parse_workflow_status)
        .transpose()?;

    match state
        .orchestrator
        .list_workflows(&auth.namespace_id, status)
        .await
    {
        Ok(workflows) => Ok(Json(serde_json::to_value(workflows).unwrap())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_queue(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    let auth = authenticate_request(&state, &headers).await?;
    match state
        .orchestrator
        .get_queue_status(&auth.namespace_id)
        .await
    {
        Ok(status) => Ok(Json(serde_json::to_value(status).unwrap())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn delete_queue_item(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let auth = authenticate_request(&state, &headers).await?;
    match state
        .orchestrator
        .remove_queued_workflow_instance(&auth.namespace_id, &id)
        .await
    {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn purge_queue(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    let auth = authenticate_request(&state, &headers).await?;
    match state
        .orchestrator
        .purge_queued_workflow_instances(&auth.namespace_id)
        .await
    {
        Ok(purged) => Ok(Json(json!({
            "status": "purged",
            "purged": purged,
        }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_task_result(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((workflow_instance_id, task_id)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    let auth = authenticate_request(&state, &headers).await?;
    match state
        .orchestrator
        .get_task_result(&auth.namespace_id, &workflow_instance_id, &task_id)
        .await
    {
        Ok(result) => Ok(Json(serde_json::to_value(result).unwrap())),
        Err(error) if error.to_string().contains("not found") => Err(StatusCode::NOT_FOUND),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::env_auth::EnvAuthAdapter;
    use crate::adapters::fake_executor::FakeExecutor;
    use crate::adapters::memory_workflow_queue::MemoryWorkflowQueue;
    use crate::adapters::storage::memory_storage::MemoryStorage;
    use crate::adapters::worker_pool::WorkerPool;
    use crate::core::orchestrator::Orchestrator;
    use crate::ports::storage::StoragePort;
    use std::sync::Arc;

    fn test_state(storage: Arc<MemoryStorage>) -> AppState {
        AppState {
            orchestrator: Arc::new(Orchestrator::new(
                storage,
                Arc::new(FakeExecutor::new()),
                Arc::new(MemoryWorkflowQueue::new(10)),
            )),
            worker_pool: WorkerPool::new(),
            auth: Arc::new(EnvAuthAdapter::from_config("token-a=namespace-a").unwrap()),
        }
    }

    fn auth_headers(token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        );
        headers
    }

    fn workflow_def(id: &str) -> WorkflowDef {
        WorkflowDef {
            id: id.to_string(),
            tasks: vec![],
            data_bindings: vec![],
        }
    }

    #[tokio::test]
    async fn protected_handler_rejects_missing_and_invalid_tokens() {
        let state = test_state(Arc::new(MemoryStorage::new()));

        let missing = create_workflow_def(
            State(state.clone()),
            HeaderMap::new(),
            Json(workflow_def("workflow-1")),
        )
        .await;
        assert_eq!(missing.unwrap_err(), StatusCode::UNAUTHORIZED);

        let invalid = create_workflow_def(
            State(state),
            auth_headers("wrong-token"),
            Json(workflow_def("workflow-1")),
        )
        .await;
        assert_eq!(invalid.unwrap_err(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn protected_handler_uses_authenticated_namespace() {
        let storage = Arc::new(MemoryStorage::new());
        let state = test_state(storage.clone());

        let Json(response) = create_workflow_def(
            State(state),
            auth_headers("token-a"),
            Json(workflow_def("workflow-1")),
        )
        .await
        .unwrap();

        assert_eq!(response, json!({ "status": "created", "id": "workflow-1" }));
        assert!(
            storage
                .get_workflow_def("namespace-a", "workflow-1")
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            storage
                .get_workflow_def("default", "workflow-1")
                .await
                .unwrap()
                .is_none()
        );
    }
}
