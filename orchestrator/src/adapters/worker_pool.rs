use crate::core::models::TaskDef;
use crate::ports::executor::ExecutionResult;
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex, Notify, RwLock, oneshot};
use tokio::time;
use tracing::{debug, warn};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerRegistration {
    pub worker_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDispatch {
    pub task_id: String,
    pub task: TaskDef,
    #[serde(default)]
    pub inputs: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub result: WorkerExecutionResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkerExecutionResult {
    Success { output: serde_json::Value },
    InputNeeded { description: String },
    Failure { reason: String },
}

impl From<ExecutionResult> for WorkerExecutionResult {
    fn from(value: ExecutionResult) -> Self {
        match value {
            ExecutionResult::Success(output) => Self::Success { output },
            ExecutionResult::InputNeeded(description) => Self::InputNeeded { description },
            ExecutionResult::Failure(reason) => Self::Failure { reason },
        }
    }
}

impl From<WorkerExecutionResult> for ExecutionResult {
    fn from(value: WorkerExecutionResult) -> Self {
        match value {
            WorkerExecutionResult::Success { output } => Self::Success(output),
            WorkerExecutionResult::InputNeeded { description } => Self::InputNeeded(description),
            WorkerExecutionResult::Failure { reason } => Self::Failure(reason),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerResponse {
    RegistrationAck { worker_id: String },
    NoTask,
    TaskDispatch(TaskDispatch),
}

#[derive(Debug)]
struct WorkerState {
    current_task_id: Option<String>,
}

#[derive(Debug)]
struct PendingTask {
    dispatch: TaskDispatch,
    result_tx: oneshot::Sender<WorkerExecutionResult>,
    claimed_tx: oneshot::Sender<()>,
}

/// Manages a pool of workers and task dispatching. 
/// Tasks are dispatched to workers in the order they are received, but only when a worker claims them. 
/// This allows for more efficient handling of task timeouts, as a task will not start its timeout countdown until it has been claimed by a worker.
/// 
#[derive(Debug, Clone, Default)]
pub struct WorkerPool {
    // Registered workers and their current state
    workers: Arc<RwLock<HashMap<String, WorkerState>>>,
    // Queue of pending tasks waiting to be claimed by workers
    pending_tasks: Arc<Mutex<VecDeque<PendingTask>>>,
    // Map of in-flight task IDs to the worker ID that claimed them
    in_flight_tasks: Arc<Mutex<HashMap<String, String>>>,
    // Map of task IDs to the oneshot sender waiting for their result
    result_waiters: Arc<Mutex<HashMap<String, oneshot::Sender<WorkerExecutionResult>>>>,
    // Atomic counter for generating unique task dispatch IDs
    next_dispatch_id: Arc<AtomicU64>,
    // Notify workers when a new task is available for claiming 
    task_available: Arc<Notify>,
}

impl WorkerPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register_worker(&self, registration: WorkerRegistration) {
        let worker_id = registration.worker_id.clone();
        self.workers.write().await.insert(
            worker_id.clone(),
            WorkerState {
                current_task_id: None,
            },
        );

        debug!(%worker_id, "registered worker");
    }

    /// Enqueues a task to be claimed by a worker. The task will not start its timeout until it has been claimed. 
    /// If the task is not claimed within the specified timeout, it will be removed from the pending queue and an error will be returned.
    pub async fn enqueue_task(
        &self,
        task: &TaskDef,
        inputs: &[serde_json::Value],
        timeout: Duration,
    ) -> anyhow::Result<ExecutionResult> {
        let task_id = format!(
            "{}-{}",
            task.id,
            self.next_dispatch_id.fetch_add(1, Ordering::Relaxed)
        );

        // Create a oneshot channel to receive the task result and another to signal when the task has been claimed
        let (result_tx, result_rx) = oneshot::channel();

        // The claimed_tx channel is used to signal when a worker has claimed the task. 
        // This allows us to start the timeout countdown only after the task has been claimed, 
        // preventing premature timeouts while the task is still waiting to be claimed.
        let (claimed_tx, claimed_rx) = oneshot::channel();

        self.pending_tasks.lock().await.push_back(PendingTask {
            dispatch: TaskDispatch {
                task_id: task_id.clone(),
                task: task.clone(),
                inputs: inputs.to_vec(),
            },
            result_tx,
            claimed_tx,
        });

        // Notify workers that a new task is available for claiming
        self.task_available.notify_one();

        if claimed_rx.await.is_err() {
            self.remove_pending_task(&task_id).await;
            return Err(anyhow!("task {task_id} was cancelled before claim"));
        }

        match time::timeout(timeout, result_rx).await {
            Ok(Ok(result)) => Ok(result.into()),
            Ok(Err(_)) => {
                self.remove_pending_task(&task_id).await;
                Err(anyhow!("task {task_id} was cancelled before completion"))
            }
            Err(_) => {
                self.remove_pending_task(&task_id).await;
                self.result_waiters.lock().await.remove(&task_id);
                if let Some(worker_id) = self.in_flight_tasks.lock().await.remove(&task_id) {
                    self.mark_worker_idle_if_current(&worker_id, &task_id).await;
                }
                Err(anyhow!("task {task_id} timed out after {:?}", timeout))
            }
        }
    }

    pub async fn claim_task(
        &self,
        worker_id: &str,
        timeout: Duration,
    ) -> anyhow::Result<Option<TaskDispatch>> {
        if !self.workers.read().await.contains_key(worker_id) {
            anyhow::bail!("worker {worker_id} is not registered");
        }

        let claim = async {
            loop {
                let notified = self.task_available.notified();
                if let Some(task) = self.pending_tasks.lock().await.pop_front() {
                    let task_id = task.dispatch.task_id.clone();
                    self.result_waiters
                        .lock()
                        .await
                        .insert(task_id.clone(), task.result_tx);
                    self.in_flight_tasks
                        .lock()
                        .await
                        .insert(task_id.clone(), worker_id.to_string());
                    self.mark_worker_running(worker_id, &task_id).await;
                    let _ = task.claimed_tx.send(());
                    return task.dispatch;
                }
                notified.await;
            }
        };

        Ok(time::timeout(timeout, claim).await.ok())
    }

    pub async fn complete_task_result(
        &self,
        worker_id: &str,
        result: TaskResult,
    ) -> anyhow::Result<()> {
        let task_id = result.task_id.clone();
        let waiter = self.result_waiters.lock().await.remove(&task_id);
        self.in_flight_tasks.lock().await.remove(&task_id);
        self.mark_worker_idle_if_current(worker_id, &task_id).await;

        if let Some(waiter) = waiter {
            let _ = waiter.send(result.result);
        } else {
            warn!(worker_id = %worker_id, task_id = %task_id, "ignoring late or untracked worker task result");
        }

        Ok(())
    }

    pub async fn worker_for_task(&self, task_id: &str) -> Option<String> {
        self.in_flight_tasks.lock().await.get(task_id).cloned()
    }

    async fn remove_pending_task(&self, task_id: &str) {
        let mut pending_tasks = self.pending_tasks.lock().await;
        if let Some(index) = pending_tasks
            .iter()
            .position(|task| task.dispatch.task_id == task_id)
        {
            pending_tasks.remove(index);
        }
    }

    async fn mark_worker_running(&self, worker_id: &str, task_id: &str) {
        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(worker_id) {
            worker.current_task_id = Some(task_id.to_string());
        }
    }

    async fn mark_worker_idle_if_current(&self, worker_id: &str, task_id: &str) {
        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(worker_id) {
            if worker.current_task_id.as_deref() == Some(task_id) {
                worker.current_task_id = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{TaskDef, TaskTypeDef};
    use serde_json::json;

    #[tokio::test]
    async fn worker_claims_queued_task_and_completes_result() {
        let pool = WorkerPool::new();
        pool.register_worker(WorkerRegistration {
            worker_id: "worker-1".to_string(),
        })
        .await;

        let task = test_task("task-1");
        let execution_pool = pool.clone();
        let execution = tokio::spawn(async move {
            execution_pool
                .enqueue_task(&task, &[], Duration::from_secs(5))
                .await
        });

        let claimed = pool
            .claim_task("worker-1", Duration::from_secs(5))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(claimed.task.id, "task-1");

        pool.complete_task_result(
            "worker-1",
            TaskResult {
                task_id: claimed.task_id,
                result: WorkerExecutionResult::Success {
                    output: json!({"worker": "worker-1"}),
                },
            },
        )
        .await
        .unwrap();

        assert_eq!(
            unwrap_success(execution.await.unwrap().unwrap()),
            json!({"worker": "worker-1"})
        );
    }

    #[tokio::test]
    async fn queued_task_does_not_time_out_before_claim() {
        let pool = WorkerPool::new();
        pool.register_worker(WorkerRegistration {
            worker_id: "worker-1".to_string(),
        })
        .await;

        let task = test_task("task-1");
        let execution_pool = pool.clone();
        let execution = tokio::spawn(async move {
            execution_pool
                .enqueue_task(&task, &[], Duration::from_millis(10))
                .await
        });

        time::sleep(Duration::from_millis(30)).await;
        assert!(!execution.is_finished());

        let claimed = pool
            .claim_task("worker-1", Duration::from_secs(5))
            .await
            .unwrap()
            .unwrap();

        pool.complete_task_result(
            "worker-1",
            TaskResult {
                task_id: claimed.task_id,
                result: WorkerExecutionResult::Success {
                    output: json!({"worker": "worker-1"}),
                },
            },
        )
        .await
        .unwrap();

        assert_eq!(
            unwrap_success(execution.await.unwrap().unwrap()),
            json!({"worker": "worker-1"})
        );
    }

    #[tokio::test]
    async fn worker_claims_next_queued_task() {
        let pool = WorkerPool::new();
        pool.register_worker(WorkerRegistration {
            worker_id: "worker-1".to_string(),
        })
        .await;

        let task = test_task("task-1");
        let execution_pool = pool.clone();
        let execution = tokio::spawn(async move {
            execution_pool
                .enqueue_task(&task, &[], Duration::from_secs(5))
                .await
        });

        let claimed = pool
            .claim_task("worker-1", Duration::from_millis(10))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(claimed.task.id, "task-1");
        assert!(!execution.is_finished());
        execution.abort();
    }

    #[tokio::test]
    async fn unregistered_worker_cannot_claim_task() {
        let pool = WorkerPool::new();
        let error = pool
            .claim_task("missing-worker", Duration::from_millis(10))
            .await
            .unwrap_err();

        assert!(error.to_string().contains("not registered"));
    }

    fn test_task(id: &str) -> TaskDef {
        TaskDef {
            id: id.to_string(),
            kind: TaskTypeDef::Function {
                dependencies: vec![],
                code: "return 1".to_string(),
            },
            timeout_secs: None,
            input_schemas: vec![],
            output_schema: None,
            expected_side_effects: vec![],
            required_credentials: vec![],
        }
    }

    fn unwrap_success(result: ExecutionResult) -> serde_json::Value {
        match result {
            ExecutionResult::Success(output) => output,
            other => panic!("expected success, got {other:?}"),
        }
    }
}
