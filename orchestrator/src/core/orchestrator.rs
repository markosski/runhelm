use crate::core::engine::WorkflowEngine;
use crate::core::models::{
    TaskDef, TaskStatus, WorkflowDef, WorkflowInstance, WorkflowStatus, WorkflowStatusReport,
};
use crate::ports::executor::ExecutorPort;
use crate::ports::storage::{StoragePort, TaskResult};
use std::sync::Arc;

/// The application layer for the orchestrator.
/// It coordinates between the workflow engine, storage, and executors.
pub struct Orchestrator {
    engine: WorkflowEngine,
    storage: Arc<dyn StoragePort + Send + Sync>,
    executor: Arc<dyn ExecutorPort + Send + Sync>,
}

impl Orchestrator {
    pub fn new(
        storage: Arc<dyn StoragePort + Send + Sync>,
        executor: Arc<dyn ExecutorPort + Send + Sync>,
    ) -> Self {
        let engine = WorkflowEngine::new(storage.clone(), executor.clone());
        Self {
            engine,
            storage,
            executor,
        }
    }

    /// Registers a new workflow definition.
    pub async fn create_workflow_def(&self, def: WorkflowDef) -> anyhow::Result<()> {
        self.storage.save_workflow_def(def).await
    }

    /// Retrieves a workflow definition by ID.
    pub async fn get_workflow_def(&self, id: &str) -> anyhow::Result<Option<WorkflowDef>> {
        self.storage.get_workflow_def(id).await
    }

    /// Creates a new workflow instance.
    pub async fn create_workflow_instance(&self, instance: WorkflowInstance) -> anyhow::Result<()> {
        self.storage.save_workflow_instance(instance).await
    }

    /// Returns a status report for a workflow instance.
    pub async fn get_workflow_status(
        &self,
        id: &str,
    ) -> anyhow::Result<Option<WorkflowStatusReport>> {
        self.engine.get_workflow_status(id).await
    }

    pub async fn get_task_result(
        &self,
        workflow_instance_id: &str,
        task_id: &str,
    ) -> anyhow::Result<TaskResult> {
        self.storage
            .get_task_result(workflow_instance_id, task_id)
            .await
    }

    /// Starts or resumes execution of a workflow instance.
    pub async fn run_workflow(&self, instance_id: String) -> anyhow::Result<()> {
        self.engine.run_workflow_instance(instance_id).await
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

    /// Executes a single task in isolation, bypasses workflow orchestration.
    /// Useful for testing individual task types or executors.
    pub async fn execute_task_isolated(
        &self,
        task: &TaskDef,
        inputs: &[serde_json::Value],
    ) -> anyhow::Result<crate::ports::executor::ExecutionResult> {
        self.executor.execute(task, inputs).await
    }
}
