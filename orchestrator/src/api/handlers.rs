use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use std::time::Duration;
use tracing::{error, info};

use crate::adapters::task_dispatcher::{WorkerExecutionResult, WorkerTaskResult};
use crate::adapters::worker_registry::WorkerRegistration;
use crate::api::models::WorkerResponse;
use crate::core::models::FunctionDef;
use crate::core::workflow::models::{WorkflowDef, WorkflowStatus};
use crate::ports::task_dispatch::ExecutionResult;
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
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let workflow_def_id = workflow_def.id.clone();

    state
        .workflow_service
        .create_workflow_def(workflow_def)
        .await
        .map_err(|error| {
            let message = error.to_string();
            let (code, response_message) = if message.contains("cannot be overwritten") {
                (StatusCode::CONFLICT, message.clone())
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to register workflow definition".to_string(),
                )
            };
            error!("Error while registering workflow: {}", error);
            (code, Json(json!({ "error": response_message })))
        })?;
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
    Json(payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let Some(pinned_worker_host) = state.worker_registry.select_eligible_host().await else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let input = trigger_payload_input(payload);
    let instance_id = state
        .workflow_service
        .create_workflow_instance_for_def(&workflow_def_id, pinned_worker_host.clone(), input)
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

    info!(
        %instance_id,
        pinned_host_id = %pinned_worker_host.0,
        "Created queued workflow instance"
    );

    Ok(Json(json!({
        "status": "queued",
        "id": instance_id,
        "pinned_host_id": pinned_worker_host
    })))
}

fn trigger_payload_input(payload: Value) -> Option<Value> {
    match payload {
        Value::Null => None,
        value => Some(value),
    }
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

pub async fn get_workflow_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state.workflow_service.list_workflow_events(&id).await {
        Ok(Some(events)) => Ok(Json(serde_json::to_value(events).unwrap())),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn pause_workflow(
    State(state): State<AppState>,
    Path(workflow_instance_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state
        .orchestrator
        .pause_workflow_instance(&workflow_instance_id)
        .await
    {
        Ok(()) => Ok(Json(json!({
            "status": "paused",
            "workflow_instance_id": workflow_instance_id,
        }))),
        Err(error) if error.to_string().contains("not found") => Err(StatusCode::NOT_FOUND),
        Err(error) if error.to_string().contains("cannot be paused") => Err(StatusCode::CONFLICT),
        Err(error) => {
            tracing::error!(%workflow_instance_id, %error, "workflow pause request failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn resume_workflow(
    State(state): State<AppState>,
    Path(workflow_instance_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state
        .orchestrator
        .resume_workflow_instance(&workflow_instance_id)
        .await
    {
        Ok(()) => Ok(Json(json!({
            "status": "queued",
            "workflow_instance_id": workflow_instance_id,
        }))),
        // TODO: Consider implementing better error modes so we don't have to string match
        Err(error) if error.to_string().contains("not found") => Err(StatusCode::NOT_FOUND),
        Err(error) if error.to_string().contains("not paused") => Err(StatusCode::CONFLICT),
        Err(error) => {
            tracing::error!(%workflow_instance_id, %error, "workflow resume request failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn pause_active_workflows(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    match state.orchestrator.pause_active_workflow_instances().await {
        Ok(workflow_instance_ids) => Ok(Json(json!({
            "status": "paused",
            "count": workflow_instance_ids.len(),
            "workflow_instance_ids": workflow_instance_ids,
        }))),
        Err(error) => {
            tracing::error!(%error, "bulk workflow pause request failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn resume_paused_workflows(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    match state.orchestrator.resume_paused_workflow_instances().await {
        Ok(workflow_instance_ids) => Ok(Json(json!({
            "status": "queued",
            "count": workflow_instance_ids.len(),
            "workflow_instance_ids": workflow_instance_ids,
        }))),
        Err(error) => {
            tracing::error!(%error, "bulk workflow resume request failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Deserialize)]
pub struct SubmitHumanInputRequest {
    input: Value,
}

pub async fn submit_human_input(
    State(state): State<AppState>,
    Path((workflow_instance_id, task_id)): Path<(String, String)>,
    Json(payload): Json<SubmitHumanInputRequest>,
) -> Result<Json<Value>, StatusCode> {
    match state
        .workflow_service
        .submit_human_input(&workflow_instance_id, &task_id, payload.input)
        .await
    {
        Ok(task_attempt_id) => {
            state
                .orchestrator
                .enqueue_workflow_instance(workflow_instance_id.clone())
                .await
                .map_err(|error| {
                    tracing::error!(%workflow_instance_id, %task_id, %error, "failed to enqueue workflow after human input submission");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;

            Ok(Json(json!({
                "status": "queued",
                "workflow_instance_id": workflow_instance_id,
                "task_attempt_id": task_attempt_id,
            })))
        }
        Err(error) if error.to_string().contains("not found") => Err(StatusCode::NOT_FOUND),
        Err(error) if error.to_string().contains("not waiting for input") => {
            Err(StatusCode::CONFLICT)
        }
        Err(error) if error.to_string().contains("not an Agent task") => Err(StatusCode::CONFLICT),
        Err(error)
            if error
                .to_string()
                .contains("already has a materialized continuation") =>
        {
            Err(StatusCode::CONFLICT)
        }
        Err(error) => {
            tracing::error!(%workflow_instance_id, %task_id, %error, "human input submission failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn retry_task(
    State(state): State<AppState>,
    Path((workflow_instance_id, task_id)): Path<(String, String)>,
    Query(query): Query<RetryTaskQuery>,
) -> Result<Json<Value>, StatusCode> {
    let result = if query.force.unwrap_or(false) {
        state
            .orchestrator
            .force_retry_workflow_task(&workflow_instance_id, &task_id, &state.worker_registry)
            .await
    } else {
        state
            .orchestrator
            .retry_workflow_task(&workflow_instance_id, &task_id)
            .await
    };

    match result {
        Ok(result) => Ok(Json(json!({
            "status": "queued",
            "workflow_instance_id": workflow_instance_id,
            "task_attempt_id": result.task_attempt_id,
            "pinned_host_id": result.pinned_host_id,
            "local_context_may_be_lost": result.local_context_may_be_lost,
        }))),
        Err(error) if error.to_string().contains("not found") => Err(StatusCode::NOT_FOUND),
        Err(error) if error.to_string().contains("not failed") => Err(StatusCode::CONFLICT),
        Err(error) if error.to_string().contains("no eligible retry host") => {
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
        Err(error) => {
            tracing::error!(%workflow_instance_id, %task_id, %error, "task retry request failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Deserialize)]
pub struct RetryTaskQuery {
    force: Option<bool>,
}

#[derive(Deserialize)]
pub struct WorkflowListQuery {
    status: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
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

    match state
        .workflow_service
        .list_workflows(status, query.limit, query.cursor.as_deref())
        .await
    {
        Ok(workflows) => Ok(Json(serde_json::to_value(workflows).unwrap())),
        Err(error) if error.to_string().contains("invalid workflow list cursor") => {
            Err(StatusCode::BAD_REQUEST)
        }
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
    state.worker_registry.register_worker(registration).await;
    let heartbeat_policy = state.worker_registry.heartbeat_policy();

    Ok(Json(
        serde_json::to_value(WorkerResponse::RegistrationAck {
            worker_id,
            heartbeat_interval_ms: heartbeat_policy.heartbeat_interval_ms,
        })
        .unwrap(),
    ))
}

pub async fn heartbeat_worker(
    State(state): State<AppState>,
    Json(registration): Json<WorkerRegistration>,
) -> Result<Json<Value>, StatusCode> {
    let worker_id = registration.worker_id.clone();
    state
        .worker_registry
        .tick_worker_heartbeat(registration)
        .await;

    Ok(Json(json!({
        "status": "accepted",
        "worker_id": worker_id
    })))
}

pub async fn claim_worker_task(
    State(state): State<AppState>,
    Json(payload): Json<WorkerClaimRequest>,
) -> Result<Json<Value>, StatusCode> {
    let worker = state
        .worker_registry
        .worker_identity_for_claim(&payload.worker_id)
        .await
        .map_err(|error| {
            if error.to_string().contains("not registered") {
                StatusCode::NOT_FOUND
            } else if error.to_string().contains("missed heartbeat") {
                StatusCode::SERVICE_UNAVAILABLE
            } else {
                tracing::error!(worker_id = %payload.worker_id, %error, "worker failed claim validation");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

    match state
        .task_dispatcher
        .claim_task(worker, Duration::from_secs(30))
        .await
    {
        Ok(Some(dispatch)) => Ok(Json(
            serde_json::to_value(WorkerResponse::TaskDispatch(dispatch)).unwrap(),
        )),
        Ok(None) => Ok(Json(serde_json::to_value(WorkerResponse::NoTask).unwrap())),
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
    let worker_id = state
        .task_dispatcher
        .worker_for_active_dispatch(&task_id)
        .await;

    if let Some(worker_id) = worker_id {
        state
            .task_dispatcher
            .complete_task_result(&worker_id, WorkerTaskResult { task_id, result })
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
        "inputneeded" | "input-needed" | "input_needed" => Ok(WorkflowStatus::InputNeeded),
        "completed" => Ok(WorkflowStatus::Completed),
        "failed" => Ok(WorkflowStatus::Failed),
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake_task_dispatcher::FakeTaskDispatcher;
    use crate::adapters::memory_storage::MemoryStorage;
    use crate::adapters::memory_workflow_queue::MemoryWorkflowQueue;
    use crate::adapters::task_dispatcher::TaskDispatcher;
    use crate::adapters::worker_registry::WorkerRegistry;
    use crate::api::router::AppState;
    use crate::core::function_service::FunctionService;
    use crate::core::models::{TaskInstance, TaskSatisfactionStatus, TaskStatus, TaskTypeDef};
    use crate::core::orchestrator::Orchestrator;
    use crate::core::workflow::models::{
        WorkerHostId, WorkflowDef, WorkflowInstance, WorkflowStatus,
    };
    use crate::core::workflow::workflow_service::WorkflowService;
    use crate::ports::storage::StoragePort;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn failed_task() -> TaskInstance {
        TaskInstance {
            task_def_id: "taska".to_string(),
            status: TaskStatus::Failed,
            satisfaction_status: TaskSatisfactionStatus::Unsatisfied,
            human_input: None,
            input_data: vec![],
            input_mapping: vec![],
            output_data: Some(json!({"stale": true})),
            generation_index: 1,
            verifier_metadata: None,
        }
    }

    fn input_needed_task() -> TaskInstance {
        TaskInstance {
            task_def_id: "taska".to_string(),
            status: TaskStatus::InputNeeded {
                input_request: "need approval".to_string(),
            },
            satisfaction_status: TaskSatisfactionStatus::Pending,
            human_input: None,
            input_data: vec![],
            input_mapping: vec![],
            output_data: None,
            generation_index: 1,
            verifier_metadata: None,
        }
    }

    fn workflow_instance(
        id: &str,
        status: WorkflowStatus,
        pinned_worker_host: Option<WorkerHostId>,
        task: TaskInstance,
    ) -> WorkflowInstance {
        WorkflowInstance {
            id: id.to_string(),
            workflow_def_id: "workflow-1".to_string(),
            version: 0,
            status,
            trigger_input: None,
            pinned_worker_host,
            tasks: HashMap::from([("taska[1]".to_string(), task)]),
            verifier_states: HashMap::new(),
        }
    }

    fn app_state(storage: Arc<MemoryStorage>, worker_registry: WorkerRegistry) -> AppState {
        AppState {
            orchestrator: Arc::new(Orchestrator::new(
                storage.clone(),
                Arc::new(FakeTaskDispatcher::new()),
                Arc::new(MemoryWorkflowQueue::new(10)),
            )),
            workflow_service: Arc::new(WorkflowService::new(storage.clone())),
            function_service: Arc::new(FunctionService::new(storage)),
            worker_registry,
            task_dispatcher: Arc::new(TaskDispatcher::new()),
        }
    }

    async fn save_failed_workflow(storage: &Arc<MemoryStorage>) {
        storage
            .save_workflow_instance(
                0,
                vec![],
                workflow_instance(
                    "failed-workflow",
                    WorkflowStatus::Failed,
                    Some(WorkerHostId::new("host-a")),
                    failed_task(),
                ),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn create_workflow_def_api_returns_conflict_with_new_id_guidance() {
        let storage = Arc::new(MemoryStorage::new());
        let state = app_state(storage.clone(), WorkerRegistry::new());
        let workflow_def = WorkflowDef {
            id: "workflow-1".to_string(),
            tasks: vec![],
            data_bindings: vec![],
        };
        state
            .workflow_service
            .create_workflow_def(workflow_def.clone())
            .await
            .unwrap();
        storage
            .save_workflow_instance(
                0,
                vec![],
                WorkflowInstance {
                    id: "workflow-instance".to_string(),
                    workflow_def_id: "workflow-1".to_string(),
                    version: 0,
                    status: WorkflowStatus::Completed,
                    trigger_input: None,
                    pinned_worker_host: None,
                    tasks: HashMap::new(),
                    verifier_states: HashMap::new(),
                },
            )
            .await
            .unwrap();

        let (status, Json(response)) = create_workflow_def(State(state), Json(workflow_def))
            .await
            .unwrap_err();

        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(
            response["error"],
            "workflow definition workflow-1 already has workflow instances and cannot be overwritten; register under a new ID, for example workflow-1_v2"
        );
    }

    #[tokio::test]
    async fn retry_task_api_returns_queued_state_and_preserves_pin() {
        let storage = Arc::new(MemoryStorage::new());
        save_failed_workflow(&storage).await;
        let state = app_state(storage.clone(), WorkerRegistry::new());

        let Json(response) = retry_task(
            State(state.clone()),
            Path(("failed-workflow".to_string(), "taska".to_string())),
            Query(RetryTaskQuery { force: None }),
        )
        .await
        .unwrap();

        assert_eq!(response["status"], "queued");
        assert_eq!(response["workflow_instance_id"], "failed-workflow");
        assert_eq!(response["task_attempt_id"], "taska[1]");
        assert_eq!(response["pinned_host_id"], "host-a");
        assert_eq!(response["local_context_may_be_lost"], false);

        let saved = storage
            .get_workflow_instance("failed-workflow")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.status, WorkflowStatus::Pending);
        assert_eq!(saved.pinned_worker_host, Some(WorkerHostId::new("host-a")));
    }

    #[tokio::test]
    async fn force_retry_task_api_without_capacity_leaves_workflow_in_failed_state() {
        let storage = Arc::new(MemoryStorage::new());
        save_failed_workflow(&storage).await;
        let state = app_state(storage.clone(), WorkerRegistry::new());

        let error = retry_task(
            State(state.clone()),
            Path(("failed-workflow".to_string(), "taska".to_string())),
            Query(RetryTaskQuery { force: Some(true) }),
        )
        .await
        .unwrap_err();

        assert_eq!(error, StatusCode::SERVICE_UNAVAILABLE);

        let Json(status) = get_workflow_instance(State(state), Path("failed-workflow".to_string()))
            .await
            .unwrap();
        assert_eq!(status["status"], "Failed");
        assert_eq!(status["tasks"][0]["status"], "Failed");

        let saved = storage
            .get_workflow_instance("failed-workflow")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.status, WorkflowStatus::Failed);
        assert_eq!(saved.pinned_worker_host, Some(WorkerHostId::new("host-a")));
    }

    #[tokio::test]
    async fn pause_and_resume_workflow_api_update_queue() {
        let storage = Arc::new(MemoryStorage::new());
        let state = app_state(storage.clone(), WorkerRegistry::new());
        storage
            .save_workflow_instance(
                0,
                vec![],
                workflow_instance(
                    "active-workflow",
                    WorkflowStatus::Pending,
                    Some(WorkerHostId::new("host-a")),
                    input_needed_task(),
                ),
            )
            .await
            .unwrap();
        state
            .orchestrator
            .enqueue_workflow_instance("active-workflow".to_string())
            .await
            .unwrap();

        let Json(paused) =
            pause_workflow(State(state.clone()), Path("active-workflow".to_string()))
                .await
                .unwrap();

        assert_eq!(paused["status"], "paused");
        assert_eq!(paused["workflow_instance_id"], "active-workflow");
        assert_eq!(
            storage
                .get_workflow_instance("active-workflow")
                .await
                .unwrap()
                .unwrap()
                .status,
            WorkflowStatus::Paused
        );
        assert!(
            state
                .orchestrator
                .get_queue_status()
                .await
                .unwrap()
                .pending
                .is_empty()
        );

        let Json(resumed) =
            resume_workflow(State(state.clone()), Path("active-workflow".to_string()))
                .await
                .unwrap();

        assert_eq!(resumed["status"], "queued");
        assert_eq!(resumed["workflow_instance_id"], "active-workflow");
        assert_eq!(
            state.orchestrator.get_queue_status().await.unwrap().pending,
            vec!["active-workflow".to_string()]
        );
    }

    #[tokio::test]
    async fn bulk_pause_and_resume_workflows_api_returns_affected_ids() {
        let storage = Arc::new(MemoryStorage::new());
        let state = app_state(storage.clone(), WorkerRegistry::new());
        for (id, status) in [
            ("pending-workflow", WorkflowStatus::Pending),
            ("running-workflow", WorkflowStatus::Running),
            ("paused-workflow", WorkflowStatus::Paused),
            ("completed-workflow", WorkflowStatus::Completed),
        ] {
            storage
                .save_workflow_instance(
                    0,
                    vec![],
                    workflow_instance(id, status, None, input_needed_task()),
                )
                .await
                .unwrap();
        }

        let Json(paused) = pause_active_workflows(State(state.clone())).await.unwrap();
        let mut paused_ids: Vec<String> = paused["workflow_instance_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect();
        paused_ids.sort();
        assert_eq!(paused["status"], "paused");
        assert_eq!(paused["count"], 2);
        assert_eq!(
            paused_ids,
            vec![
                "pending-workflow".to_string(),
                "running-workflow".to_string(),
            ]
        );

        let Json(resumed) = resume_paused_workflows(State(state.clone())).await.unwrap();
        let mut resumed_ids: Vec<String> = resumed["workflow_instance_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect();
        resumed_ids.sort();
        assert_eq!(resumed["status"], "queued");
        assert_eq!(resumed["count"], 3);
        assert_eq!(
            resumed_ids,
            vec![
                "paused-workflow".to_string(),
                "pending-workflow".to_string(),
                "running-workflow".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn submit_human_input_api_materializes_continuation_and_queues_workflow() {
        let storage = Arc::new(MemoryStorage::new());
        let state = app_state(storage.clone(), WorkerRegistry::new());
        state
            .workflow_service
            .create_workflow_def(WorkflowDef {
                id: "workflow-1".to_string(),
                tasks: vec![crate::core::models::TaskDef {
                    id: "taska".to_string(),
                    kind: TaskTypeDef::Agent {
                        model_id: "model".to_string(),
                        provider_url: "provider".to_string(),
                        prompt: "prompt".to_string(),
                        tools: vec![],
                        skills: vec![],
                        ask: true,
                        schema_failure_retry_times: 0.into(),
                        reuse_session: true,
                    },
                    control: None,
                    timeout_secs: None,
                    input_schemas: vec![],
                    output_schema: None,
                    workspace: None,
                    required_credentials: vec![],
                }],
                data_bindings: vec![],
            })
            .await
            .unwrap();
        storage
            .save_workflow_instance(
                0,
                vec![],
                workflow_instance(
                    "input-needed-workflow",
                    WorkflowStatus::InputNeeded,
                    Some(WorkerHostId::new("host-a")),
                    input_needed_task(),
                ),
            )
            .await
            .unwrap();

        let Json(response) = submit_human_input(
            State(state.clone()),
            Path(("input-needed-workflow".to_string(), "taska".to_string())),
            Json(SubmitHumanInputRequest {
                input: json!({"approved": true}),
            }),
        )
        .await
        .unwrap();

        assert_eq!(response["status"], "queued");
        assert_eq!(response["task_attempt_id"], "taska[2]");

        let saved = storage
            .get_workflow_instance("input-needed-workflow")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.status, WorkflowStatus::Pending);
        assert_eq!(saved.pinned_worker_host, Some(WorkerHostId::new("host-a")));
        assert_eq!(
            saved.tasks["taska[2]"].human_input,
            Some(json!({"approved": true}))
        );
        assert_eq!(
            state.orchestrator.get_queue_status().await.unwrap().pending,
            vec!["input-needed-workflow".to_string()]
        );
    }

    #[tokio::test]
    async fn list_workflows_api_filters_by_valid_status_query() {
        let storage = Arc::new(MemoryStorage::new());
        let state = app_state(storage.clone(), WorkerRegistry::new());
        storage
            .save_workflow_instance(
                0,
                vec![],
                workflow_instance(
                    "input-needed-workflow",
                    WorkflowStatus::InputNeeded,
                    None,
                    input_needed_task(),
                ),
            )
            .await
            .unwrap();
        storage
            .save_workflow_instance(
                0,
                vec![],
                workflow_instance(
                    "failed-workflow",
                    WorkflowStatus::Failed,
                    None,
                    failed_task(),
                ),
            )
            .await
            .unwrap();

        let Json(response) = list_workflows(
            State(state),
            Query(WorkflowListQuery {
                status: Some("InputNeeded".to_string()),
                limit: None,
                cursor: None,
            }),
        )
        .await
        .unwrap();

        let workflows = response["workflows"].as_array().unwrap();
        assert_eq!(workflows.len(), 1);
        assert_eq!(workflows[0]["id"], "input-needed-workflow");
        assert_eq!(workflows[0]["status"], "InputNeeded");
    }
}
