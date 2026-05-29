use crate::api::models::{WorkflowQueueStatus, WorkflowStatusReport};
use crate::core::engine::WorkflowEngine;
use crate::core::function_resolution::resolve_task_function_ref;
use crate::core::models::{ExecutionMetadata, TaskDef, TaskStatus, WorkflowStatus};
use crate::ports::executor::ExecutorPort;
use crate::ports::storage::StoragePort;
use crate::ports::workflow_queue::WorkflowQueuePort;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{error, info};

/// The application layer for the orchestrator.
/// It coordinates between the workflow engine, storage, and executors.
pub struct Orchestrator {
    engine: WorkflowEngine,
    storage: Arc<dyn StoragePort + Send + Sync>,
    executor: Arc<dyn ExecutorPort + Send + Sync>,
    workflow_queue: Arc<dyn WorkflowQueuePort + Send + Sync>,
}

impl Orchestrator {
    pub fn new(
        storage: Arc<dyn StoragePort + Send + Sync>,
        executor: Arc<dyn ExecutorPort + Send + Sync>,
        workflow_queue: Arc<dyn WorkflowQueuePort + Send + Sync>,
    ) -> Self {
        let engine = WorkflowEngine::new(storage.clone(), executor.clone());
        Self {
            engine,
            storage,
            executor,
            workflow_queue,
        }
    }

    /// Finds a task in a registered workflow definition and executes it directly.
    pub async fn execute_workflow_task_isolated(
        &self,
        workflow_def_id: &str,
        task_id: &str,
        inputs: &[serde_json::Value],
    ) -> anyhow::Result<Option<crate::ports::executor::ExecutionResult>> {
        let Some(def) = self.storage.get_workflow_def(workflow_def_id).await? else {
            return Ok(None);
        };

        let Some(task) = def.tasks.into_iter().find(|task| task.id == task_id) else {
            return Ok(None);
        };

        self.execute_task_isolated(&task, inputs).await.map(Some)
    }

    /// Adds a workflow instance to the execution queue.
    pub async fn enqueue_workflow_instance(&self, instance_id: String) -> anyhow::Result<()> {
        self.workflow_queue.enqueue(instance_id).await
    }

    /// Returns queued and currently running workflow instance IDs.
    pub async fn get_queue_status(&self) -> anyhow::Result<WorkflowQueueStatus> {
        let pending = self.workflow_queue.pending_ids().await?;

        Ok(WorkflowQueueStatus { pending })
    }

    /// Removes a pending workflow instance from the queue.
    pub async fn remove_queued_workflow_instance(&self, instance_id: &str) -> anyhow::Result<bool> {
        self.workflow_queue.remove(instance_id).await
    }

    /// Removes all pending workflow instances from the queue.
    pub async fn purge_queued_workflow_instances(&self) -> anyhow::Result<Vec<String>> {
        self.workflow_queue.purge().await
    }

    /// Returns a status report for a workflow instance.
    pub async fn get_workflow_status(
        &self,
        id: &str,
    ) -> anyhow::Result<Option<WorkflowStatusReport>> {
        self.engine.get_workflow_status(id).await
    }

    /// Starts or resumes execution of a workflow instance.
    pub async fn run_workflow(&self, instance_id: String) -> anyhow::Result<()> {
        self.engine.run_workflow_instance(instance_id).await
    }

    /// Continuously consumes queued workflow instances and runs up to `max_concurrent_workflows`.
    pub async fn run_scheduler(self: Arc<Self>, max_concurrent_workflows: usize) {
        let max_concurrent_workflows = max_concurrent_workflows.max(1);
        let permits = Arc::new(Semaphore::new(max_concurrent_workflows));
        info!(max_concurrent_workflows, "workflow scheduler started");

        loop {
            let permit = match Arc::clone(&permits).acquire_owned().await {
                Ok(permit) => permit,
                Err(error) => {
                    error!(%error, "workflow scheduler semaphore closed");
                    break;
                }
            };

            let instance_id = match self.workflow_queue.dequeue().await {
                Ok(instance_id) => instance_id,
                Err(error) => {
                    error!(%error, "workflow scheduler failed to dequeue workflow instance");
                    drop(permit);
                    break;
                }
            };

            let orchestrator = Arc::clone(&self);
            tokio::spawn(async move {
                let workflow_instance_id = instance_id.clone();
                if let Err(error) = orchestrator.run_workflow(instance_id).await {
                    error!(
                        %workflow_instance_id,
                        error = ?error,
                        "workflow execution failed"
                    );
                }
                drop(permit);
            });
        }
    }

    /// Reconciles in-flight workflow state after an orchestrator restart.
    ///
    /// Storage is the source of truth. Any task left Running from a previous
    /// process is moved back to Pending so it can be dispatched again.
    pub async fn synchronize_startup_tasks(&self) -> anyhow::Result<usize> {
        let mut recovered = 0;

        for mut instance in self.storage.list_active_workflow_instances().await? {
            let mut changed = false;

            for task in instance.tasks.values_mut() {
                if task.status == TaskStatus::Running {
                    task.status = TaskStatus::Pending;
                    changed = true;
                }
            }

            if instance.status == WorkflowStatus::Running {
                instance.status = WorkflowStatus::Pending;
                changed = true;
            }

            if changed {
                self.storage.save_workflow_instance(instance).await?;
                recovered += 1;
            }
        }

        Ok(recovered)
    }

    /// Requeues all active workflow instances found in storage.
    pub async fn enqueue_active_workflow_instances(&self) -> anyhow::Result<usize> {
        let instances = self.storage.list_active_workflow_instances().await?;
        let count = instances.len();

        for instance in instances {
            self.enqueue_workflow_instance(instance.id).await?;
        }

        Ok(count)
    }

    /// Executes a single task in isolation, bypasses workflow orchestration.
    /// Useful for testing individual task types or executors.
    pub async fn execute_task_isolated(
        &self,
        task: &TaskDef,
        inputs: &[serde_json::Value],
    ) -> anyhow::Result<crate::ports::executor::ExecutionResult> {
        let task = self.resolve_task_function_ref(task).await?;
        self.executor
            .execute(&task, inputs, &ExecutionMetadata::default())
            .await
    }

    async fn resolve_task_function_ref(&self, task: &TaskDef) -> anyhow::Result<TaskDef> {
        resolve_task_function_ref(self.storage.as_ref(), task).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake_executor::FakeExecutor;
    use crate::adapters::memory_storage::MemoryStorage;
    use crate::adapters::memory_workflow_queue::MemoryWorkflowQueue;
    use crate::core::function_service::FunctionService;
    use crate::core::models::{
        DataBinding, FunctionDef, FunctionTaskDef, TaskTypeDef, WorkflowDef, WorkflowInstance,
        verifier_decision_schema,
    };
    use crate::core::workflow_service::WorkflowService;
    use crate::ports::executor::ExecutionResult;
    use crate::ports::storage::{StoragePort, TaskResult};
    use async_trait::async_trait;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::{Duration, sleep};

    fn orchestrator() -> Orchestrator {
        Orchestrator::new(
            Arc::new(MemoryStorage::new()),
            Arc::new(FakeExecutor::new()),
            Arc::new(MemoryWorkflowQueue::new(10)),
        )
    }

    fn orchestrator_with_services() -> (Orchestrator, WorkflowService, FunctionService) {
        let storage = Arc::new(MemoryStorage::new());
        (
            Orchestrator::new(
                storage.clone(),
                Arc::new(FakeExecutor::new()),
                Arc::new(MemoryWorkflowQueue::new(10)),
            ),
            WorkflowService::new(storage.clone()),
            FunctionService::new(storage),
        )
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
            _task: &TaskDef,
            _inputs: &[serde_json::Value],
            _metadata: &ExecutionMetadata,
        ) -> anyhow::Result<ExecutionResult> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);
            sleep(self.delay).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
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
        let orchestrator = Arc::new(Orchestrator::new(storage.clone(), executor.clone(), queue));
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
                .save_workflow_instance(workflow_instance(id, id))
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
            TaskResult::SuccessWithMetadata {
                input,
                output,
                metadata,
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
            TaskResult::SuccessWithMetadata {
                input,
                output,
                metadata,
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
        let orchestrator = Orchestrator::new(storage.clone(), Arc::new(FakeExecutor::new()), queue);

        let mut running = workflow_instance("running-workflow", "workflow-1");
        running.status = WorkflowStatus::Running;
        storage.save_workflow_instance(running).await.unwrap();

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
}
