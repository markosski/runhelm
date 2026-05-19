use crate::core::engine::WorkflowEngine;
use crate::core::function_resolution::resolve_task_function_ref;
use crate::core::models::{
    FunctionDef, TaskDef, TaskStatus, WorkflowDef, WorkflowInstance, WorkflowList,
    WorkflowQueueStatus, WorkflowStatus, WorkflowStatusReport, WorkflowSummary,
};
use crate::ports::executor::ExecutorPort;
use crate::ports::storage::{StoragePort, TaskResult};
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

    /// Registers a new workflow definition.
    pub async fn create_workflow_def(
        &self,
        namespace_id: &str,
        def: WorkflowDef,
    ) -> anyhow::Result<()> {
        self.storage.save_workflow_def(namespace_id, def).await
    }

    /// Retrieves a workflow definition by ID.
    pub async fn get_workflow_def(
        &self,
        namespace_id: &str,
        id: &str,
    ) -> anyhow::Result<Option<WorkflowDef>> {
        self.storage.get_workflow_def(namespace_id, id).await
    }

    /// Registers a reusable function definition.
    pub async fn create_function_def(
        &self,
        namespace_id: &str,
        def: FunctionDef,
    ) -> anyhow::Result<()> {
        self.storage.save_function_def(namespace_id, def).await
    }

    /// Deletes a reusable function definition.
    pub async fn delete_function_def(&self, namespace_id: &str, id: &str) -> anyhow::Result<bool> {
        self.storage.delete_function_def(namespace_id, id).await
    }

    /// Finds a task in a registered workflow definition and executes it directly.
    pub async fn execute_workflow_task_isolated(
        &self,
        namespace_id: &str,
        workflow_def_id: &str,
        task_id: &str,
        inputs: &[serde_json::Value],
    ) -> anyhow::Result<Option<crate::ports::executor::ExecutionResult>> {
        let Some(def) = self
            .storage
            .get_workflow_def(namespace_id, workflow_def_id)
            .await?
        else {
            return Ok(None);
        };

        let Some(task) = def.tasks.into_iter().find(|task| task.id == task_id) else {
            return Ok(None);
        };

        self.execute_task_isolated(namespace_id, &task, inputs)
            .await
            .map(Some)
    }

    /// Creates a new workflow instance.
    pub async fn create_workflow_instance(
        &self,
        namespace_id: &str,
        instance: WorkflowInstance,
    ) -> anyhow::Result<()> {
        self.storage
            .save_workflow_instance(namespace_id, instance)
            .await
    }

    /// Adds a workflow instance to the execution queue.
    pub async fn enqueue_workflow_instance(
        &self,
        namespace_id: String,
        instance_id: String,
    ) -> anyhow::Result<()> {
        self.workflow_queue.enqueue(namespace_id, instance_id).await
    }

    /// Returns queued and currently running workflow instance IDs.
    pub async fn get_queue_status(
        &self,
        namespace_id: &str,
    ) -> anyhow::Result<WorkflowQueueStatus> {
        let pending = self.workflow_queue.pending_ids(namespace_id).await?;

        Ok(WorkflowQueueStatus { pending })
    }

    /// Removes a pending workflow instance from the queue.
    pub async fn remove_queued_workflow_instance(
        &self,
        namespace_id: &str,
        instance_id: &str,
    ) -> anyhow::Result<bool> {
        self.workflow_queue.remove(namespace_id, instance_id).await
    }

    /// Removes all pending workflow instances from the queue.
    pub async fn purge_queued_workflow_instances(
        &self,
        namespace_id: &str,
    ) -> anyhow::Result<Vec<String>> {
        self.workflow_queue.purge(namespace_id).await
    }

    /// Returns a status report for a workflow instance.
    pub async fn get_workflow_status(
        &self,
        namespace_id: &str,
        id: &str,
    ) -> anyhow::Result<Option<WorkflowStatusReport>> {
        self.engine.get_workflow_status(namespace_id, id).await
    }

    pub async fn list_workflows(
        &self,
        namespace_id: &str,
        status: Option<WorkflowStatus>,
    ) -> anyhow::Result<WorkflowList> {
        let mut workflows: Vec<WorkflowSummary> = self
            .storage
            .list_workflow_instances(namespace_id)
            .await?
            .into_iter()
            .filter(|instance| {
                status
                    .as_ref()
                    .is_none_or(|status| instance.status == *status)
            })
            .map(|instance| WorkflowSummary {
                id: instance.id,
                workflow_def_id: instance.workflow_def_id,
                status: instance.status,
            })
            .collect();

        workflows.sort_by(|left, right| left.id.cmp(&right.id));

        Ok(WorkflowList { workflows })
    }

    pub async fn get_task_result(
        &self,
        namespace_id: &str,
        workflow_instance_id: &str,
        task_id: &str,
    ) -> anyhow::Result<TaskResult> {
        self.storage
            .get_task_result(namespace_id, workflow_instance_id, task_id)
            .await
    }

    /// Starts or resumes execution of a workflow instance.
    pub async fn run_workflow(
        &self,
        namespace_id: String,
        instance_id: String,
    ) -> anyhow::Result<()> {
        self.engine
            .run_workflow_instance(namespace_id, instance_id)
            .await
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

            let queued = match self.workflow_queue.dequeue().await {
                Ok(queued) => queued,
                Err(error) => {
                    error!(%error, "workflow scheduler failed to dequeue workflow instance");
                    drop(permit);
                    break;
                }
            };

            let orchestrator = Arc::clone(&self);
            tokio::spawn(async move {
                let namespace_id = queued.namespace_id.clone();
                let workflow_instance_id = queued.workflow_instance_id.clone();
                if let Err(error) = orchestrator
                    .run_workflow(queued.namespace_id, queued.workflow_instance_id)
                    .await
                {
                    error!(
                        %namespace_id,
                        %workflow_instance_id,
                        %error,
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

        for mut item in self.storage.list_active_workflow_instances().await? {
            let namespace_id = item.namespace_id;
            let instance = &mut item.instance;
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
                self.storage
                    .save_workflow_instance(&namespace_id, item.instance)
                    .await?;
                recovered += 1;
            }
        }

        Ok(recovered)
    }

    /// Requeues all active workflow instances found in storage.
    pub async fn enqueue_active_workflow_instances(&self) -> anyhow::Result<usize> {
        let instances = self.storage.list_active_workflow_instances().await?;
        let count = instances.len();

        for item in instances {
            self.enqueue_workflow_instance(item.namespace_id, item.instance.id)
                .await?;
        }

        Ok(count)
    }

    /// Executes a single task in isolation, bypasses workflow orchestration.
    /// Useful for testing individual task types or executors.
    pub async fn execute_task_isolated(
        &self,
        namespace_id: &str,
        task: &TaskDef,
        inputs: &[serde_json::Value],
    ) -> anyhow::Result<crate::ports::executor::ExecutionResult> {
        let task = self.resolve_task_function_ref(namespace_id, task).await?;
        self.executor.execute(&task, inputs).await
    }

    async fn resolve_task_function_ref(
        &self,
        namespace_id: &str,
        task: &TaskDef,
    ) -> anyhow::Result<TaskDef> {
        resolve_task_function_ref(self.storage.as_ref(), namespace_id, task).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake_executor::FakeExecutor;
    use crate::adapters::memory_workflow_queue::MemoryWorkflowQueue;
    use crate::adapters::storage::memory_storage::MemoryStorage;
    use crate::core::models::{FunctionDef, FunctionTaskDef, TaskTypeDef};
    use crate::ports::executor::ExecutionResult;
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
            timeout_secs: None,
            input_schemas: vec![],
            output_schema: Some(json!({
                "type": "object",
                "required": ["ok"],
                "properties": {
                    "ok": { "type": "boolean" }
                }
            })),
            expected_side_effects: vec![],
            required_credentials: vec![],
        }
    }

    fn function_ref_task(id: &str, reference: &str) -> TaskDef {
        TaskDef {
            id: id.to_string(),
            kind: TaskTypeDef::Function(FunctionTaskDef::Ref {
                reference: reference.to_string(),
            }),
            timeout_secs: None,
            input_schemas: vec![],
            output_schema: Some(json!({
                "type": "object",
                "required": ["ok"],
                "properties": {
                    "ok": { "type": "boolean" }
                }
            })),
            expected_side_effects: vec![],
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
        }
    }

    #[tokio::test]
    async fn execute_workflow_task_isolated_finds_registered_task() {
        let orchestrator = orchestrator();
        orchestrator
            .create_workflow_def("default", workflow("workflow-1", vec![task("task-a")]))
            .await
            .unwrap();

        let result = orchestrator
            .execute_workflow_task_isolated("default", "workflow-1", "task-a", &[])
            .await
            .unwrap();

        assert_eq!(
            result,
            Some(ExecutionResult::Success(json!({ "ok": false })))
        );
    }

    #[tokio::test]
    async fn execute_workflow_task_isolated_scopes_task_lookup_to_workflow_def() {
        let orchestrator = orchestrator();
        orchestrator
            .create_workflow_def("default", workflow("workflow-1", vec![task("task-a")]))
            .await
            .unwrap();
        orchestrator
            .create_workflow_def("default", workflow("workflow-2", vec![task("task-a")]))
            .await
            .unwrap();

        let result = orchestrator
            .execute_workflow_task_isolated("default", "workflow-2", "task-a", &[])
            .await
            .unwrap();

        assert_eq!(
            result,
            Some(ExecutionResult::Success(json!({ "ok": false })))
        );
    }

    #[tokio::test]
    async fn execute_workflow_task_isolated_resolves_registered_function_ref() {
        let orchestrator = orchestrator();
        orchestrator
            .create_function_def(
                "default",
                FunctionDef {
                    id: "function-a".to_string(),
                    dependencies: vec![],
                    code: "export default async function run() { return {}; }".to_string(),
                },
            )
            .await
            .unwrap();
        orchestrator
            .create_workflow_def(
                "default",
                workflow(
                    "workflow-1",
                    vec![function_ref_task("task-a", "function-a")],
                ),
            )
            .await
            .unwrap();

        let result = orchestrator
            .execute_workflow_task_isolated("default", "workflow-1", "task-a", &[])
            .await
            .unwrap();

        assert_eq!(
            result,
            Some(ExecutionResult::Success(json!({ "ok": false })))
        );
    }

    #[tokio::test]
    async fn execute_workflow_task_isolated_errors_for_missing_function_ref() {
        let orchestrator = orchestrator();
        orchestrator
            .create_workflow_def(
                "default",
                workflow(
                    "workflow-1",
                    vec![function_ref_task("task-a", "missing-function")],
                ),
            )
            .await
            .unwrap();

        let error = orchestrator
            .execute_workflow_task_isolated("default", "workflow-1", "task-a", &[])
            .await
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("Function definition not found: missing-function")
        );
    }

    #[tokio::test]
    async fn function_refs_do_not_cross_namespaces() {
        let orchestrator = orchestrator();
        orchestrator
            .create_function_def(
                "namespace-b",
                FunctionDef {
                    id: "function-a".to_string(),
                    dependencies: vec![],
                    code: "export default async function run() { return {}; }".to_string(),
                },
            )
            .await
            .unwrap();
        orchestrator
            .create_workflow_def(
                "namespace-a",
                workflow(
                    "workflow-1",
                    vec![function_ref_task("task-a", "function-a")],
                ),
            )
            .await
            .unwrap();

        let error = orchestrator
            .execute_workflow_task_isolated("namespace-a", "workflow-1", "task-a", &[])
            .await
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("Function definition not found: function-a")
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
                .save_workflow_def(
                    "default",
                    workflow(
                        id,
                        vec![TaskDef {
                            output_schema: None,
                            ..task("task-a")
                        }],
                    ),
                )
                .await
                .unwrap();
            storage
                .save_workflow_instance("default", workflow_instance(id, id))
                .await
                .unwrap();
            orchestrator
                .enqueue_workflow_instance("default".to_string(), id.to_string())
                .await
                .unwrap();
        }

        for _ in 0..20 {
            let mut completed = 0;
            for id in ["workflow-1", "workflow-2", "workflow-3"] {
                let instance = storage
                    .get_workflow_instance("default", id)
                    .await
                    .unwrap()
                    .unwrap();
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
        let orchestrator = orchestrator();
        orchestrator
            .create_workflow_def("default", workflow("workflow-1", vec![task("task-a")]))
            .await
            .unwrap();

        let result = orchestrator
            .execute_workflow_task_isolated("default", "workflow-1", "task-a", &[])
            .await
            .unwrap();

        assert!(matches!(result, Some(ExecutionResult::Success(_))));
    }

    #[tokio::test]
    async fn queue_status_lists_pending_workflows() {
        let storage = Arc::new(MemoryStorage::new());
        let queue = Arc::new(MemoryWorkflowQueue::new(10));
        let orchestrator = Orchestrator::new(storage.clone(), Arc::new(FakeExecutor::new()), queue);

        let mut running = workflow_instance("running-workflow", "workflow-1");
        running.status = WorkflowStatus::Running;
        storage
            .save_workflow_instance("default", running)
            .await
            .unwrap();

        orchestrator
            .enqueue_workflow_instance("default".to_string(), "pending-workflow".to_string())
            .await
            .unwrap();

        assert_eq!(
            orchestrator.get_queue_status("default").await.unwrap(),
            WorkflowQueueStatus {
                pending: vec!["pending-workflow".to_string()],
            }
        );
    }

    #[tokio::test]
    async fn enqueue_active_workflow_instances_preserves_namespace() {
        let storage = Arc::new(MemoryStorage::new());
        let queue = Arc::new(MemoryWorkflowQueue::new(10));
        let orchestrator = Orchestrator::new(storage.clone(), Arc::new(FakeExecutor::new()), queue);

        let mut active = workflow_instance("active-workflow", "workflow-1");
        active.status = WorkflowStatus::Running;
        storage
            .save_workflow_instance("namespace-a", active)
            .await
            .unwrap();

        assert_eq!(
            orchestrator
                .enqueue_active_workflow_instances()
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            orchestrator
                .get_queue_status("namespace-a")
                .await
                .unwrap()
                .pending,
            vec!["active-workflow".to_string()]
        );
        assert!(
            orchestrator
                .get_queue_status("default")
                .await
                .unwrap()
                .pending
                .is_empty()
        );
    }

    #[tokio::test]
    async fn remove_and_purge_affect_pending_queue_only() {
        let orchestrator = orchestrator();

        orchestrator
            .enqueue_workflow_instance("default".to_string(), "workflow-1".to_string())
            .await
            .unwrap();
        orchestrator
            .enqueue_workflow_instance("default".to_string(), "workflow-2".to_string())
            .await
            .unwrap();

        assert!(
            orchestrator
                .remove_queued_workflow_instance("default", "workflow-1")
                .await
                .unwrap()
        );
        assert_eq!(
            orchestrator
                .purge_queued_workflow_instances("default")
                .await
                .unwrap(),
            vec!["workflow-2".to_string()]
        );
        assert!(
            orchestrator
                .get_queue_status("default")
                .await
                .unwrap()
                .pending
                .is_empty()
        );
    }

    #[tokio::test]
    async fn list_workflows_filters_by_status() {
        let storage = Arc::new(MemoryStorage::new());
        let orchestrator = Orchestrator::new(
            storage.clone(),
            Arc::new(FakeExecutor::new()),
            Arc::new(MemoryWorkflowQueue::new(10)),
        );

        let mut completed = workflow_instance("completed-workflow", "workflow-1");
        completed.status = WorkflowStatus::Completed;
        let mut running = workflow_instance("running-workflow", "workflow-1");
        running.status = WorkflowStatus::Running;
        storage
            .save_workflow_instance("default", completed)
            .await
            .unwrap();
        storage
            .save_workflow_instance("default", running)
            .await
            .unwrap();

        assert_eq!(
            orchestrator
                .list_workflows("default", Some(WorkflowStatus::Running))
                .await
                .unwrap(),
            WorkflowList {
                workflows: vec![WorkflowSummary {
                    id: "running-workflow".to_string(),
                    workflow_def_id: "workflow-1".to_string(),
                    status: WorkflowStatus::Running,
                }],
            }
        );
    }
}
