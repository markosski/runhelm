use crate::api::models::{WorkflowQueueStatus, WorkflowStatusReport};
use crate::core::engine::WorkflowEngine;
use crate::core::function_service::resolve_task_function_ref;
use crate::core::models::{ExecutionMetadata, TaskDef, TaskStatus};
use crate::core::workflow::models::WorkflowStatus;
use crate::core::workspace_manager::WorkspaceManager;
use crate::ports::executor::ExecutorPort;
use crate::ports::storage::StoragePort;
use crate::ports::workflow_queue::WorkflowQueuePort;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Semaphore;
use tracing::{error, info};

#[cfg(test)]
#[path = "orchestrator_tests.rs"]
mod tests;

/// The application layer for the orchestrator.
/// It coordinates between the workflow engine, storage, and executors.
pub struct Orchestrator {
    engine: WorkflowEngine,
    storage: Arc<dyn StoragePort + Send + Sync>,
    executor: Arc<dyn ExecutorPort + Send + Sync>,
    workflow_queue: Arc<dyn WorkflowQueuePort + Send + Sync>,
    workspace_manager: Arc<WorkspaceManager>,
}

impl Orchestrator {
    pub fn new(
        storage: Arc<dyn StoragePort + Send + Sync>,
        executor: Arc<dyn ExecutorPort + Send + Sync>,
        workflow_queue: Arc<dyn WorkflowQueuePort + Send + Sync>,
        workspace_manager: Arc<WorkspaceManager>,
    ) -> Self {
        let engine =
            WorkflowEngine::new(storage.clone(), executor.clone(), workspace_manager.clone());

        Self {
            engine,
            storage,
            executor,
            workflow_queue,
            workspace_manager,
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

        self.execute_task_isolated_with_id(
            isolated_workflow_task_execution_id(workflow_def_id, task_id)?,
            &task,
            inputs,
        )
        .await
        .map(Some)
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

    async fn execute_task_isolated_with_id(
        &self,
        isolated_execution_id: String,
        task: &TaskDef,
        inputs: &[serde_json::Value],
    ) -> anyhow::Result<crate::ports::executor::ExecutionResult> {
        let task = self.resolve_task_function_ref(task).await?;
        let workspace_path = self
            .workspace_manager
            .create_or_time_stamp_workspace(&isolated_execution_id, &task)?;
        self.executor
            .execute(
                &isolated_execution_id,
                &task,
                inputs,
                &ExecutionMetadata::default(),
                &workspace_path,
            )
            .await
    }

    async fn resolve_task_function_ref(&self, task: &TaskDef) -> anyhow::Result<TaskDef> {
        resolve_task_function_ref(self.storage.as_ref(), task).await
    }
}

fn isolated_workflow_task_execution_id(
    workflow_def_id: &str,
    task_id: &str,
) -> anyhow::Result<String> {
    Ok(format!(
        "isolated-{workflow_def_id}-{task_id}-{}",
        timestamp_nanos()?
    ))
}

fn timestamp_nanos() -> anyhow::Result<u128> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos())
}
