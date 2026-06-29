use crate::api::models::{WorkflowQueueStatus, WorkflowStatusReport};
use crate::core::engine::WorkflowEngine;
use crate::core::function_service::resolve_task_function_ref;
use crate::core::models::{ExecutionMetadata, TaskDef, TaskStatus};
use crate::core::workflow::events::WorkflowInstanceEvent;
use crate::core::workflow::models::{
    StartupWorkflowDiscovery, TaskDispatchConstraints, WorkerHostId, WorkflowInfo, WorkflowStatus,
};
use crate::core::workflow::state_manager::WorkflowStateManager;
use crate::ports::executor::ExecutorPort;
use crate::ports::storage::{
    StoragePort, WorkflowInfoCursor, WorkflowInfoListRequest, WorkflowInfoPageRequest,
    WorkflowInstanceFilter,
};
use crate::ports::workflow_queue::WorkflowQueuePort;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Semaphore;
use tracing::{error, info, warn};

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
    pub async fn run_workflow_queue(self: Arc<Self>, max_concurrent_workflows: usize) {
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

        let state_manager = WorkflowStateManager::new(Arc::clone(&self.storage));

        for info in self.list_active_workflow_info().await?.runnable {
            let Some(instance) = self.storage.get_workflow_instance(&info.id).await? else {
                continue;
            };
            let changed = instance.status == WorkflowStatus::Running
                || instance
                    .tasks
                    .values()
                    .any(|task| task.status == TaskStatus::Running);

            if changed {
                state_manager
                    .commit_events(
                        &info.id,
                        vec![WorkflowInstanceEvent::StartupRecoveryApplied],
                    )
                    .await?;
                recovered += 1;
            }
        }

        Ok(recovered)
    }

    /// Requeues all active workflow instances found in storage.
    pub async fn enqueue_active_workflow_instances(&self) -> anyhow::Result<usize> {
        let infos = self.list_active_workflow_info().await?.runnable;
        let count = infos.len();

        for info in infos {
            self.enqueue_workflow_instance(info.id).await?;
        }

        Ok(count)
    }

    /// Fails non-terminal workflow instances pinned to hosts that heartbeat
    /// policy has declared lost. The existing pin remains on the snapshot so
    /// later retry behavior can decide whether to preserve or reassign it.
    pub async fn fail_workflows_pinned_to_lost_hosts(
        &self,
        lost_hosts: &[WorkerHostId],
    ) -> anyhow::Result<usize> {
        if lost_hosts.is_empty() {
            return Ok(0);
        }

        let lost_hosts = lost_hosts.iter().cloned().collect::<HashSet<_>>();
        let state_manager = WorkflowStateManager::new(Arc::clone(&self.storage));
        let active = self.list_active_workflow_info().await?;
        let mut failed = 0;

        for info in active.runnable.into_iter().chain(active.blocked) {
            let Some(instance) = self.storage.get_workflow_instance(&info.id).await? else {
                continue;
            };
            let Some(pinned_host) = &instance.pinned_worker_host else {
                continue;
            };
            if is_terminal_workflow_status(&instance.status) || !lost_hosts.contains(pinned_host) {
                continue;
            }
            let pinned_host_id = pinned_host.0.clone();

            state_manager
                .commit_events_for_instance(
                    instance,
                    vec![WorkflowInstanceEvent::WorkflowStatusChanged {
                        status: WorkflowStatus::Failed,
                    }],
                )
                .await?;
            failed += 1;
            warn!(
                workflow_instance_id = %info.id,
                pinned_host_id = %pinned_host_id,
                "marked workflow failed because pinned host was declared lost"
            );
        }

        Ok(failed)
    }

    async fn execute_task_isolated_with_id(
        &self,
        isolated_execution_id: String,
        task: &TaskDef,
        inputs: &[serde_json::Value],
    ) -> anyhow::Result<crate::ports::executor::ExecutionResult> {
        let task = self.resolve_task_function_ref(task).await?;
        self.executor
            .execute(
                &isolated_execution_id,
                &task,
                inputs,
                &ExecutionMetadata::default(),
                &TaskDispatchConstraints::default(),
            )
            .await
    }

    async fn resolve_task_function_ref(&self, task: &TaskDef) -> anyhow::Result<TaskDef> {
        resolve_task_function_ref(self.storage.as_ref(), task).await
    }

    async fn list_active_workflow_info(&self) -> anyhow::Result<StartupWorkflowDiscovery> {
        let mut cursor: Option<WorkflowInfoCursor> = None;
        let mut runnable_infos: Vec<WorkflowInfo> = Vec::new();
        let mut blocked_infos: Vec<WorkflowInfo> = Vec::new();

        loop {
            let page = self
                .storage
                .list_workflow_info(WorkflowInfoListRequest {
                    filters: all_nonterminal_workflow_filter(),
                    page: WorkflowInfoPageRequest { limit: 100, cursor },
                })
                .await?;

            let runnable: Vec<WorkflowInfo> = page
                .workflows
                .iter()
                .filter(|x| matches!(x.status, WorkflowStatus::Running | WorkflowStatus::Pending))
                .map(|x| x.clone())
                .collect();
            let blocked: Vec<WorkflowInfo> = page
                .workflows
                .iter()
                .filter(|x| {
                    matches!(
                        x.status,
                        WorkflowStatus::Paused | WorkflowStatus::InputNeeded
                    )
                })
                .map(|x| x.clone())
                .collect();

            runnable_infos.extend(runnable);
            blocked_infos.extend(blocked);
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }

        Ok(StartupWorkflowDiscovery {
            runnable: runnable_infos,
            blocked: blocked_infos,
        })
    }
}

fn all_nonterminal_workflow_filter() -> Vec<WorkflowInstanceFilter> {
    vec![WorkflowInstanceFilter::Statuses(vec![
        WorkflowStatus::Pending,
        WorkflowStatus::Running,
        WorkflowStatus::Paused,
        WorkflowStatus::InputNeeded,
    ])]
}

fn is_terminal_workflow_status(status: &WorkflowStatus) -> bool {
    matches!(status, WorkflowStatus::Completed | WorkflowStatus::Failed)
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
