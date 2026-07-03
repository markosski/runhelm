use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use std::time::Duration;
use tracing::{error, info};

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
        .map_err(|error| {
            let code;
            if error.to_string().contains("cannot be overwritten") {
                code = StatusCode::CONFLICT
            } else {
                code = StatusCode::INTERNAL_SERVER_ERROR
            }
            error!("Error while registering workflow: {}", error);
            code
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
    Json(_payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let Some(pinned_worker_host) = state.worker_pool.select_eligible_host().await else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let instance_id = state
        .workflow_service
        .create_workflow_instance_for_def(&workflow_def_id, pinned_worker_host.clone())
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
            .force_retry_workflow_task(&workflow_instance_id, &task_id, &state.worker_pool)
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
    state.worker_pool.register_worker(registration).await;
    let heartbeat_policy = state.worker_pool.heartbeat_policy();

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
    state.worker_pool.tick_worker_heartbeat(registration).await;

    Ok(Json(json!({
        "status": "accepted",
        "worker_id": worker_id
    })))
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
        Err(error) if error.to_string().contains("missed heartbeat") => {
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
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
    let worker_id = state.worker_pool.worker_for_active_dispatch(&task_id).await;

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
        "inputneeded" | "input-needed" | "input_needed" => Ok(WorkflowStatus::InputNeeded),
        "completed" => Ok(WorkflowStatus::Completed),
        "failed" => Ok(WorkflowStatus::Failed),
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake_executor::FakeExecutor;
    use crate::adapters::memory_storage::MemoryStorage;
    use crate::adapters::memory_workflow_queue::MemoryWorkflowQueue;
    use crate::adapters::worker_pool::WorkerPool;
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
            status,
            pinned_worker_host,
            tasks: HashMap::from([("taska[1]".to_string(), task)]),
            verifier_states: HashMap::new(),
        }
    }

    fn app_state(storage: Arc<MemoryStorage>, worker_pool: WorkerPool) -> AppState {
        AppState {
            orchestrator: Arc::new(Orchestrator::new(
                storage.clone(),
                Arc::new(FakeExecutor::new()),
                Arc::new(MemoryWorkflowQueue::new(10)),
            )),
            workflow_service: Arc::new(WorkflowService::new(storage.clone())),
            function_service: Arc::new(FunctionService::new(storage)),
            worker_pool,
        }
    }

    async fn save_failed_workflow(storage: &Arc<MemoryStorage>) {
        storage
            .commit_workflow_instance_events(
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
    async fn retry_task_api_returns_queued_state_and_preserves_pin() {
        let storage = Arc::new(MemoryStorage::new());
        save_failed_workflow(&storage).await;
        let state = app_state(storage.clone(), WorkerPool::new());

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
        let state = app_state(storage.clone(), WorkerPool::new());

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
    async fn submit_human_input_api_materializes_continuation_and_queues_workflow() {
        let storage = Arc::new(MemoryStorage::new());
        let state = app_state(storage.clone(), WorkerPool::new());
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
            .commit_workflow_instance_events(
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
        let state = app_state(storage.clone(), WorkerPool::new());
        storage
            .commit_workflow_instance_events(
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
            .commit_workflow_instance_events(
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
