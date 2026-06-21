use super::*;
use crate::adapters::fake_executor::FakeExecutor;
use crate::adapters::memory_storage::MemoryStorage;
use crate::adapters::memory_workflow_queue::MemoryWorkflowQueue;
use crate::api::models::WorkflowQueueStatus;
use crate::core::function_service::FunctionService;
use crate::core::models::{
    ExecutionMetadata, FunctionDef, FunctionTaskDef, TaskDef, TaskStatus, TaskTypeDef, Workspace,
    verifier_decision_schema,
};
use crate::core::workflow::models::{DataBinding, WorkflowDef, WorkflowInstance};
use crate::core::workflow::workflow_service::WorkflowService;
use crate::core::workspace_manager::WorkspaceManagerConfig;
use crate::ports::executor::ExecutionResult;
use crate::ports::executor::ExecutorPort;
use crate::ports::storage::{StoragePort, TaskResult};
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{Duration, sleep};

fn orchestrator() -> Orchestrator {
    Orchestrator::new(
        Arc::new(MemoryStorage::new()),
        Arc::new(FakeExecutor::new()),
        Arc::new(MemoryWorkflowQueue::new(10)),
        Arc::new(test_workspace_manager("orchestrator-default")),
    )
}

fn orchestrator_with_services() -> (Orchestrator, WorkflowService, FunctionService) {
    let storage = Arc::new(MemoryStorage::new());
    (
        Orchestrator::new(
            storage.clone(),
            Arc::new(FakeExecutor::new()),
            Arc::new(MemoryWorkflowQueue::new(10)),
            Arc::new(test_workspace_manager("orchestrator-services")),
        ),
        WorkflowService::new(storage.clone()),
        FunctionService::new(storage),
    )
}

fn temp_root(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    std::env::temp_dir().join(format!(
        "runhelm-{}-{}-{}",
        test_name,
        std::process::id(),
        nanos
    ))
}

fn test_workspace_manager(test_name: &str) -> WorkspaceManager {
    WorkspaceManager::new(WorkspaceManagerConfig {
        root: temp_root(test_name),
        ttl: Duration::from_secs(3600),
        vacuum_interval: Duration::from_secs(60),
    })
}

struct CountingExecutor {
    active: AtomicUsize,
    max_active: AtomicUsize,
    delay: Duration,
}

impl CountingExecutor {
    fn new(delay: Duration) -> Self {
        Self {
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
            delay,
        }
    }

    fn max_active(&self) -> usize {
        self.max_active.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ExecutorPort for CountingExecutor {
    async fn execute(
        &self,
        _workflow_inst_id: &str,
        _task: &TaskDef,
        _inputs: &[serde_json::Value],
        _metadata: &ExecutionMetadata,
        _workspace_path: &Path,
    ) -> anyhow::Result<ExecutionResult> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(active, Ordering::SeqCst);
        sleep(self.delay).await;
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(ExecutionResult::Success(json!({})))
    }
}

#[derive(Clone, Debug)]
struct RecordedIsolatedExecution {
    workflow_inst_id: String,
    task_id: String,
    workspace_path: PathBuf,
}

struct RecordingIsolatedExecutor {
    records: StdMutex<Vec<RecordedIsolatedExecution>>,
}

impl RecordingIsolatedExecutor {
    fn new() -> Self {
        Self {
            records: StdMutex::new(vec![]),
        }
    }

    fn records(&self) -> Vec<RecordedIsolatedExecution> {
        self.records.lock().unwrap().clone()
    }
}

#[async_trait]
impl ExecutorPort for RecordingIsolatedExecutor {
    async fn execute(
        &self,
        workflow_inst_id: &str,
        task: &TaskDef,
        _inputs: &[serde_json::Value],
        _metadata: &ExecutionMetadata,
        workspace_path: &Path,
    ) -> anyhow::Result<ExecutionResult> {
        self.records
            .lock()
            .unwrap()
            .push(RecordedIsolatedExecution {
                workflow_inst_id: workflow_inst_id.to_string(),
                task_id: task.id.clone(),
                workspace_path: workspace_path.to_path_buf(),
            });
        Ok(ExecutionResult::Success(json!({})))
    }
}

fn task(id: &str) -> TaskDef {
    TaskDef {
        id: id.to_string(),
        kind: TaskTypeDef::Function(FunctionTaskDef::Inline {
            dependencies: vec![],
            code: "export default async function run() { return {}; }".to_string(),
        }),
        control: None,
        timeout_secs: None,
        input_schemas: vec![],
        output_schema: Some(json!({
            "type": "object",
            "required": ["ok"],
            "properties": {
                "ok": { "type": "boolean" }
            }
        })),
        workspace: None,
        required_credentials: vec![],
    }
}

fn function_ref_task(id: &str, reference: &str) -> TaskDef {
    TaskDef {
        id: id.to_string(),
        kind: TaskTypeDef::Function(FunctionTaskDef::Ref {
            reference: reference.to_string(),
        }),
        control: None,
        timeout_secs: None,
        input_schemas: vec![],
        output_schema: Some(json!({
            "type": "object",
            "required": ["ok"],
            "properties": {
                "ok": { "type": "boolean" }
            }
        })),
        workspace: None,
        required_credentials: vec![],
    }
}

fn workflow(id: &str, tasks: Vec<TaskDef>) -> WorkflowDef {
    WorkflowDef {
        id: id.to_string(),
        tasks,
        data_bindings: vec![],
    }
}

fn workflow_instance(id: &str, workflow_def_id: &str) -> WorkflowInstance {
    WorkflowInstance {
        id: id.to_string(),
        workflow_def_id: workflow_def_id.to_string(),
        status: WorkflowStatus::Pending,
        tasks: HashMap::new(),
        verifier_states: HashMap::new(),
    }
}

#[tokio::test]
async fn execute_workflow_task_isolated_finds_registered_task() {
    let (orchestrator, workflow_service, _) = orchestrator_with_services();
    workflow_service
        .create_workflow_def(workflow("workflow1", vec![task("taska")]))
        .await
        .unwrap();

    let result = orchestrator
        .execute_workflow_task_isolated("workflow1", "taska", &[])
        .await
        .unwrap();

    assert_eq!(
        result,
        Some(ExecutionResult::Success(json!({ "ok": false })))
    );
}

#[tokio::test]
async fn execute_workflow_task_isolated_scopes_task_lookup_to_workflow_def() {
    let (orchestrator, workflow_service, _) = orchestrator_with_services();
    workflow_service
        .create_workflow_def(workflow("workflow1", vec![task("taska")]))
        .await
        .unwrap();
    workflow_service
        .create_workflow_def(workflow("workflow2", vec![task("taska")]))
        .await
        .unwrap();

    let result = orchestrator
        .execute_workflow_task_isolated("workflow2", "taska", &[])
        .await
        .unwrap();

    assert_eq!(
        result,
        Some(ExecutionResult::Success(json!({ "ok": false })))
    );
}

#[tokio::test]
async fn execute_workflow_task_isolated_resolves_registered_function_ref() {
    let (orchestrator, workflow_service, function_service) = orchestrator_with_services();
    function_service
        .create_function_def(FunctionDef {
            id: "functiona".to_string(),
            dependencies: vec![],
            code: "export default async function run() { return {}; }".to_string(),
        })
        .await
        .unwrap();
    workflow_service
        .create_workflow_def(workflow(
            "workflow1",
            vec![function_ref_task("taska", "functiona")],
        ))
        .await
        .unwrap();

    let result = orchestrator
        .execute_workflow_task_isolated("workflow1", "taska", &[])
        .await
        .unwrap();

    assert_eq!(
        result,
        Some(ExecutionResult::Success(json!({ "ok": false })))
    );
}

#[tokio::test]
async fn execute_workflow_task_isolated_uses_generated_isolated_execution_id() {
    let storage = Arc::new(MemoryStorage::new());
    let executor = Arc::new(RecordingIsolatedExecutor::new());
    let workflow_service = WorkflowService::new(storage.clone());
    let orchestrator = Orchestrator::new(
        storage,
        executor.clone(),
        Arc::new(MemoryWorkflowQueue::new(10)),
        Arc::new(WorkspaceManager::new(WorkspaceManagerConfig {
            root: temp_root("isolated-id"),
            ttl: Duration::from_secs(3600),
            vacuum_interval: Duration::from_secs(60),
        })),
    );

    workflow_service
        .create_workflow_def(workflow("workflow1", vec![task("taska")]))
        .await
        .unwrap();

    orchestrator
        .execute_workflow_task_isolated("workflow1", "taska", &[])
        .await
        .unwrap();

    let records = executor.records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].task_id, "taska");
    assert!(
        records[0]
            .workflow_inst_id
            .starts_with("isolated-workflow1-taska-")
    );
    assert_ne!(records[0].workflow_inst_id, "123");
}

#[tokio::test]
async fn execute_workflow_task_isolated_creates_workspace_for_isolated_execution() {
    let storage = Arc::new(MemoryStorage::new());
    let executor = Arc::new(RecordingIsolatedExecutor::new());
    let workflow_service = WorkflowService::new(storage.clone());
    let workspace_root = temp_root("isolated-workspace");
    let orchestrator = Orchestrator::new(
        storage,
        executor.clone(),
        Arc::new(MemoryWorkflowQueue::new(10)),
        Arc::new(WorkspaceManager::new(WorkspaceManagerConfig {
            root: workspace_root.clone(),
            ttl: Duration::from_secs(3600),
            vacuum_interval: Duration::from_secs(60),
        })),
    );

    workflow_service
        .create_workflow_def(workflow("workflow1", vec![task("taska")]))
        .await
        .unwrap();

    orchestrator
        .execute_workflow_task_isolated("workflow1", "taska", &[])
        .await
        .unwrap();

    let records = executor.records();
    let record = records.first().unwrap();
    assert!(record.workspace_path.is_dir());
    assert!(record.workspace_path.join(".timestamp").is_file());
    assert!(record.workspace_path.starts_with(&workspace_root));
    assert_eq!(
        record.workspace_path,
        workspace_root
            .join(&record.workflow_inst_id)
            .join("taskid-taska")
    );
}

#[tokio::test]
async fn execute_workflow_task_isolated_errors_for_missing_function_ref() {
    let (orchestrator, workflow_service, _) = orchestrator_with_services();
    workflow_service
        .create_workflow_def(workflow(
            "workflow1",
            vec![function_ref_task("taska", "missingfunction")],
        ))
        .await
        .unwrap();

    let error = orchestrator
        .execute_workflow_task_isolated("workflow1", "taska", &[])
        .await
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("Function definition not found: missingfunction")
    );
}

#[tokio::test]
async fn scheduler_limits_concurrent_workflow_execution() {
    let storage = Arc::new(MemoryStorage::new());
    let executor = Arc::new(CountingExecutor::new(Duration::from_millis(50)));
    let queue = Arc::new(MemoryWorkflowQueue::new(10));
    let orchestrator = Arc::new(Orchestrator::new(
        storage.clone(),
        executor.clone(),
        queue,
        Arc::new(test_workspace_manager("orchestrator-concurrency")),
    ));
    let scheduler = tokio::spawn(orchestrator.clone().run_scheduler(2));

    for id in ["workflow-1", "workflow-2", "workflow-3"] {
        storage
            .save_workflow_def(workflow(
                id,
                vec![TaskDef {
                    output_schema: None,
                    ..task("task-a")
                }],
            ))
            .await
            .unwrap();
        storage
            .commit_workflow_instance_events(vec![], workflow_instance(id, id))
            .await
            .unwrap();
        orchestrator
            .enqueue_workflow_instance(id.to_string())
            .await
            .unwrap();
    }

    for _ in 0..20 {
        let mut completed = 0;
        for id in ["workflow-1", "workflow-2", "workflow-3"] {
            let instance = storage.get_workflow_instance(id).await.unwrap().unwrap();
            if instance.status == WorkflowStatus::Completed {
                completed += 1;
            }
        }
        if completed == 3 {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }

    assert_eq!(executor.max_active(), 2);
    scheduler.abort();
}

#[tokio::test]
async fn isolated_workflow_task_execution_does_not_require_scheduler() {
    let (orchestrator, workflow_service, _) = orchestrator_with_services();
    workflow_service
        .create_workflow_def(workflow("workflow1", vec![task("taska")]))
        .await
        .unwrap();

    let result = orchestrator
        .execute_workflow_task_isolated("workflow1", "taska", &[])
        .await
        .unwrap();

    assert!(matches!(result, Some(ExecutionResult::Success(_))));
}

#[tokio::test]
async fn create_workflow_def_accepts_missing_input_schemas() {
    let storage = Arc::new(MemoryStorage::new());
    let workflow_service = WorkflowService::new(storage.clone());
    let workflow_def: WorkflowDef = serde_json::from_value(json!({
        "id": "workflow1",
        "tasks": [
            {
                "id": "taska",
                "kind": {
                    "Function": {
                        "dependencies": [],
                        "code": "export default async function run() { return {}; }"
                    }
                },
                "output_schema": {
                    "type": "object"
                },
                "required_credentials": []
            }
        ],
        "data_bindings": []
    }))
    .unwrap();

    workflow_service
        .create_workflow_def(workflow_def)
        .await
        .unwrap();

    let stored = storage
        .get_workflow_def("workflow1")
        .await
        .unwrap()
        .unwrap();
    assert!(stored.tasks[0].input_schemas.is_empty());
}

#[tokio::test]
async fn workflow_without_control_verifier_deserializes_and_executes() {
    let (orchestrator, workflow_service, _) = orchestrator_with_services();
    let workflow_def: WorkflowDef = serde_json::from_value(json!({
        "id": "workflow1",
        "tasks": [
            {
                "id": "taska",
                "kind": {
                    "Function": {
                        "dependencies": [],
                        "code": "export default async function run() { return {}; }"
                    }
                },
                "output_schema": {
                    "type": "object"
                },
                "required_credentials": []
            }
        ],
        "data_bindings": []
    }))
    .unwrap();

    assert!(workflow_def.tasks[0].control.is_none());

    workflow_service
        .create_workflow_def(workflow_def)
        .await
        .unwrap();
    let instance_id = workflow_service
        .create_workflow_instance_for_def("workflow1")
        .await
        .unwrap();

    orchestrator
        .run_workflow(instance_id.clone())
        .await
        .unwrap();

    let report = orchestrator
        .get_workflow_status(&instance_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(report.status, WorkflowStatus::Completed);
    assert!(report.verifier_states.is_empty());
    assert_eq!(report.tasks.len(), 1);
    assert_eq!(report.tasks[0].task_attempt_id, "taska[1]");
    assert_eq!(report.tasks[0].task_def_id, "taska");
    assert_eq!(report.tasks[0].status, TaskStatus::Completed);
    assert!(report.tasks[0].verifier_metadata.is_none());
}

#[tokio::test]
async fn get_task_result_resolves_logical_task_id_to_generation_one() {
    let (orchestrator, workflow_service, _) = orchestrator_with_services();
    workflow_service
        .create_workflow_def(workflow("workflow1", vec![task("taska")]))
        .await
        .unwrap();
    let instance_id = workflow_service
        .create_workflow_instance_for_def("workflow1")
        .await
        .unwrap();

    orchestrator
        .run_workflow(instance_id.clone())
        .await
        .unwrap();

    match workflow_service
        .get_task_result(&instance_id, "taska")
        .await
        .unwrap()
    {
        TaskResult::Success {
            input,
            output,
            metadata: Some(metadata),
        } => {
            assert_eq!(input, Vec::<serde_json::Value>::new());
            assert_eq!(output, json!({ "ok": false }));
            assert_eq!(metadata.task_def_id, "taska");
            assert_eq!(metadata.task_attempt_id, "taska[1]");
            assert_eq!(metadata.generation_index, 1);
        }
        result => panic!("expected success with metadata, got {result:?}"),
    }
}

#[tokio::test]
async fn list_task_results_returns_materialized_attempts() {
    let (orchestrator, workflow_service, _) = orchestrator_with_services();
    workflow_service
        .create_workflow_def(workflow("workflow1", vec![task("taska")]))
        .await
        .unwrap();
    let instance_id = workflow_service
        .create_workflow_instance_for_def("workflow1")
        .await
        .unwrap();

    orchestrator
        .run_workflow(instance_id.clone())
        .await
        .unwrap();

    let tasks = workflow_service
        .list_task_results(&instance_id)
        .await
        .unwrap();

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].task_attempt_id, "taska[1]");
    match &tasks[0].result {
        TaskResult::Success {
            input,
            output,
            metadata: Some(metadata),
        } => {
            assert_eq!(input, &Vec::<serde_json::Value>::new());
            assert_eq!(output, &json!({ "ok": false }));
            assert_eq!(metadata.task_def_id, "taska");
            assert_eq!(metadata.task_attempt_id, "taska[1]");
            assert_eq!(metadata.generation_index, 1);
        }
        result => panic!("expected success with metadata, got {result:?}"),
    }
}

#[tokio::test]
async fn verifier_control_accepts_function_task_and_injects_decision_schema() {
    let storage = Arc::new(MemoryStorage::new());
    let workflow_service = WorkflowService::new(storage.clone());
    let mut verifier = task("verify");
    verifier.output_schema = None;
    verifier.control = Some(crate::core::models::TaskControl {
        verifier: Some(crate::core::models::VerifierControlConfig {
            max_iterations: 2,
            on_exhausted_continue: false,
            rerun_from_task_id: None,
        }),
    });

    workflow_service
        .create_workflow_def(workflow("workflow1", vec![verifier]))
        .await
        .unwrap();

    let def = storage
        .get_workflow_def("workflow1")
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(def.tasks[0].kind, TaskTypeDef::Function(_)));
    assert_eq!(def.tasks[0].output_schema, Some(verifier_decision_schema()));
}

#[tokio::test]
async fn verifier_control_rejects_user_output_schema() {
    let workflow_service = WorkflowService::new(Arc::new(MemoryStorage::new()));
    let mut verifier = task("verify");
    verifier.control = Some(crate::core::models::TaskControl {
        verifier: Some(crate::core::models::VerifierControlConfig {
            max_iterations: 2,
            on_exhausted_continue: false,
            rerun_from_task_id: None,
        }),
    });

    let error = workflow_service
        .create_workflow_def(workflow("workflow1", vec![verifier]))
        .await
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("control.verifier and must not declare output_schema")
    );
}

#[tokio::test]
async fn create_workflow_def_normalizes_workflow_def_task_def_and_binding_ids() {
    let storage = Arc::new(MemoryStorage::new());
    let workflow_service = WorkflowService::new(storage.clone());
    let mut task_a = task("Task_A");
    task_a.workspace = Some(Workspace {
        group_name: "Repo_Cache".to_string(),
    });
    let mut task_b = task("Task-B");
    task_b.input_schemas = vec![json!({ "type": "object" })];

    workflow_service
        .create_workflow_def(WorkflowDef {
            id: "Workflow_ABC-1".to_string(),
            tasks: vec![task_a, task_b],
            data_bindings: vec![DataBinding {
                source_task_id: "Task_A".to_string(),
                target_task_id: "Task-B".to_string(),
            }],
        })
        .await
        .unwrap();

    let stored = storage
        .get_workflow_def("workflow_abc-1")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(stored.id, "workflow_abc-1");
    assert_eq!(stored.tasks[0].id, "task_a");
    assert_eq!(
        stored.tasks[0]
            .workspace
            .as_ref()
            .map(|workspace| workspace.group_name.as_str()),
        Some("repo_cache")
    );
    assert_eq!(stored.tasks[1].id, "task-b");
    assert_eq!(stored.data_bindings[0].source_task_id, "task_a");
    assert_eq!(stored.data_bindings[0].target_task_id, "task-b");
}

#[tokio::test]
async fn create_workflow_def_rejects_invalid_identifier_characters() {
    let workflow_service = WorkflowService::new(Arc::new(MemoryStorage::new()));

    let workflow_error = workflow_service
        .create_workflow_def(workflow("workflow.1", vec![task("taska")]))
        .await
        .unwrap_err();
    assert!(workflow_error.to_string().contains(
        "workflow id \"workflow.1\" must contain only ASCII alphanumeric characters, '-' or '_'"
    ));

    let task_error = workflow_service
        .create_workflow_def(workflow("workflow1", vec![task("task a")]))
        .await
        .unwrap_err();
    assert!(task_error.to_string().contains(
        "task id \"task a\" must contain only ASCII alphanumeric characters, '-' or '_'"
    ));

    let mut task_with_workspace = task("taska");
    task_with_workspace.workspace = Some(Workspace {
        group_name: "repo.cache".to_string(),
    });
    let workspace_error = workflow_service
        .create_workflow_def(workflow("workflow1", vec![task_with_workspace]))
        .await
        .unwrap_err();
    assert!(
        workspace_error
            .to_string()
            .contains("workspace group id \"repo.cache\" must contain only ASCII alphanumeric characters, '-' or '_'")
    );
}

#[tokio::test]
async fn verifier_control_rejects_invalid_rerun_from_task_id_values() {
    let workflow_service = WorkflowService::new(Arc::new(MemoryStorage::new()));

    let mut missing_target_verifier = task("verify");
    missing_target_verifier.output_schema = None;
    missing_target_verifier.control = Some(crate::core::models::TaskControl {
        verifier: Some(crate::core::models::VerifierControlConfig {
            max_iterations: 2,
            on_exhausted_continue: false,
            rerun_from_task_id: Some("missing".to_string()),
        }),
    });
    let missing_target_error = workflow_service
        .create_workflow_def(WorkflowDef {
            id: "workflow1".to_string(),
            tasks: vec![task("taska"), missing_target_verifier],
            data_bindings: vec![DataBinding {
                source_task_id: "taska".to_string(),
                target_task_id: "verify".to_string(),
            }],
        })
        .await
        .unwrap_err();
    assert!(
        missing_target_error
            .to_string()
            .contains("verifier rerun_from_task_id references unknown task id missing")
    );

    let mut downstream_target_verifier = task("taska");
    downstream_target_verifier.output_schema = None;
    downstream_target_verifier.control = Some(crate::core::models::TaskControl {
        verifier: Some(crate::core::models::VerifierControlConfig {
            max_iterations: 2,
            on_exhausted_continue: false,
            rerun_from_task_id: Some("taskb".to_string()),
        }),
    });
    let downstream_target_error = workflow_service
        .create_workflow_def(WorkflowDef {
            id: "workflow2".to_string(),
            tasks: vec![downstream_target_verifier, task("taskb")],
            data_bindings: vec![DataBinding {
                source_task_id: "taska".to_string(),
                target_task_id: "taskb".to_string(),
            }],
        })
        .await
        .unwrap_err();
    assert!(
        downstream_target_error
            .to_string()
            .contains("task taska verifier rerun task taskb is not an upstream ancestor")
    );

    let mut unrelated_target_verifier = task("verify");
    unrelated_target_verifier.output_schema = None;
    unrelated_target_verifier.control = Some(crate::core::models::TaskControl {
        verifier: Some(crate::core::models::VerifierControlConfig {
            max_iterations: 2,
            on_exhausted_continue: false,
            rerun_from_task_id: Some("taskb".to_string()),
        }),
    });
    let unrelated_target_error = workflow_service
        .create_workflow_def(WorkflowDef {
            id: "workflow3".to_string(),
            tasks: vec![task("taska"), task("taskb"), unrelated_target_verifier],
            data_bindings: vec![DataBinding {
                source_task_id: "taska".to_string(),
                target_task_id: "verify".to_string(),
            }],
        })
        .await
        .unwrap_err();
    assert!(
        unrelated_target_error
            .to_string()
            .contains("task verify verifier rerun task taskb is not an upstream ancestor")
    );
}

#[tokio::test]
async fn verifier_control_rejects_overlapping_loop_slices() {
    let workflow_service = WorkflowService::new(Arc::new(MemoryStorage::new()));
    let mut verifya = task("verifya");
    verifya.output_schema = None;
    verifya.control = Some(crate::core::models::TaskControl {
        verifier: Some(crate::core::models::VerifierControlConfig {
            max_iterations: 2,
            on_exhausted_continue: false,
            rerun_from_task_id: Some("taska".to_string()),
        }),
    });
    let mut verifyb = task("verifyb");
    verifyb.output_schema = None;
    verifyb.control = Some(crate::core::models::TaskControl {
        verifier: Some(crate::core::models::VerifierControlConfig {
            max_iterations: 2,
            on_exhausted_continue: false,
            rerun_from_task_id: Some("taskb".to_string()),
        }),
    });

    let error = workflow_service
        .create_workflow_def(WorkflowDef {
            id: "workflow1".to_string(),
            tasks: vec![task("taska"), task("taskb"), verifya, verifyb],
            data_bindings: vec![
                DataBinding {
                    source_task_id: "taska".to_string(),
                    target_task_id: "taskb".to_string(),
                },
                DataBinding {
                    source_task_id: "taskb".to_string(),
                    target_task_id: "verifya".to_string(),
                },
                DataBinding {
                    source_task_id: "taskb".to_string(),
                    target_task_id: "verifyb".to_string(),
                },
            ],
        })
        .await
        .unwrap_err();

    assert!(error.to_string().contains("verifier loop slices overlap"));
}

#[tokio::test]
async fn queue_status_lists_pending_workflows() {
    let storage = Arc::new(MemoryStorage::new());
    let queue = Arc::new(MemoryWorkflowQueue::new(10));
    let orchestrator = Orchestrator::new(
        storage.clone(),
        Arc::new(FakeExecutor::new()),
        queue,
        Arc::new(test_workspace_manager("orchestrator-queue-status")),
    );

    let mut running = workflow_instance("running-workflow", "workflow-1");
    running.status = WorkflowStatus::Running;
    storage
        .commit_workflow_instance_events(vec![], running)
        .await
        .unwrap();

    orchestrator
        .enqueue_workflow_instance("pending-workflow".to_string())
        .await
        .unwrap();

    assert_eq!(
        orchestrator.get_queue_status().await.unwrap(),
        WorkflowQueueStatus {
            pending: vec!["pending-workflow".to_string()],
        }
    );
}

#[tokio::test]
async fn remove_and_purge_affect_pending_queue_only() {
    let orchestrator = orchestrator();

    orchestrator
        .enqueue_workflow_instance("workflow-1".to_string())
        .await
        .unwrap();
    orchestrator
        .enqueue_workflow_instance("workflow-2".to_string())
        .await
        .unwrap();

    assert!(
        orchestrator
            .remove_queued_workflow_instance("workflow-1")
            .await
            .unwrap()
    );
    assert_eq!(
        orchestrator
            .purge_queued_workflow_instances()
            .await
            .unwrap(),
        vec!["workflow-2".to_string()]
    );
    assert!(
        orchestrator
            .get_queue_status()
            .await
            .unwrap()
            .pending
            .is_empty()
    );
}
