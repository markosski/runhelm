use crate::core::models::{ExecutionMetadata, TaskDef, VerifierExecutionResult};
use crate::ports::executor::ExecutionResult;
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex, Notify, RwLock, oneshot};
use tokio::time::{self, Instant};
use tracing::{debug, warn};

const TASK_TIMEOUT_MONITOR_INTERVAL: Duration = Duration::from_millis(100);

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
    #[serde(default)]
    pub execution_metadata: ExecutionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub result: WorkerExecutionResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkerExecutionResult {
    Success {
        output: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        verifier: Option<VerifierExecutionResult>,
    },
    InputNeeded {
        description: String,
    },
    Failure {
        reason: String,
    },
}

impl From<ExecutionResult> for WorkerExecutionResult {
    fn from(value: ExecutionResult) -> Self {
        match value {
            ExecutionResult::Success(output) => Self::Success {
                output,
                verifier: None,
            },
            ExecutionResult::SuccessWithVerifier { output, verifier } => Self::Success {
                output,
                verifier: Some(verifier),
            },
            ExecutionResult::InputNeeded(description) => Self::InputNeeded { description },
            ExecutionResult::Failure(reason) => Self::Failure { reason },
        }
    }
}

impl From<WorkerExecutionResult> for ExecutionResult {
    fn from(value: WorkerExecutionResult) -> Self {
        match value {
            WorkerExecutionResult::Success { output, verifier } => {
                if let Some(verifier) = verifier {
                    Self::SuccessWithVerifier { output, verifier }
                } else {
                    Self::Success(output)
                }
            }
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
    timeout: Duration,
}

#[derive(Debug)]
struct InFlightTask {
    worker_id: String,
    claimed_at: Instant,
    timeout: Duration,
}

/// Manages queued and in-flight worker tasks.
/// Tasks are claimed by workers in FIFO order, and execution timeout tracking
/// starts when a worker claims a task.
#[derive(Debug, Clone)]
pub struct WorkerPool {
    // Registered workers and their current state
    workers: Arc<RwLock<HashMap<String, WorkerState>>>,
    // Queue of pending tasks waiting to be claimed by workers
    pending_tasks: Arc<Mutex<VecDeque<PendingTask>>>,
    // Map of in-flight task IDs to their execution state
    in_flight_tasks: Arc<Mutex<HashMap<String, InFlightTask>>>,
    // Map of task IDs to the oneshot sender waiting for their result
    result_waiters: Arc<Mutex<HashMap<String, oneshot::Sender<WorkerExecutionResult>>>>,
    // Atomic counter for generating unique task dispatch IDs
    next_dispatch_id: Arc<AtomicU64>,
    // Notify workers when a new task is available for claiming
    task_available: Arc<Notify>,
}

impl Default for WorkerPool {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkerPool {
    pub fn new() -> Self {
        let pool = Self::new_without_monitor();
        pool.spawn_timeout_monitor(TASK_TIMEOUT_MONITOR_INTERVAL);
        pool
    }

    fn new_without_monitor() -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            pending_tasks: Arc::new(Mutex::new(VecDeque::new())),
            in_flight_tasks: Arc::new(Mutex::new(HashMap::new())),
            result_waiters: Arc::new(Mutex::new(HashMap::new())),
            next_dispatch_id: Arc::new(AtomicU64::new(0)),
            task_available: Arc::new(Notify::new()),
        }
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

    /// Enqueues a task and waits until it reaches a terminal worker result.
    /// Execution timeout is tracked separately after the task is claimed.
    pub async fn enqueue_task(
        &self,
        task: &TaskDef,
        inputs: &[serde_json::Value],
        timeout: Duration,
    ) -> anyhow::Result<ExecutionResult> {
        self.enqueue_task_with_metadata(task, inputs, timeout, ExecutionMetadata::default())
            .await
    }

    pub async fn enqueue_task_with_metadata(
        &self,
        task: &TaskDef,
        inputs: &[serde_json::Value],
        timeout: Duration,
        execution_metadata: ExecutionMetadata,
    ) -> anyhow::Result<ExecutionResult> {
        let task_id = format!(
            "{}-{}",
            task.id,
            self.next_dispatch_id.fetch_add(1, Ordering::Relaxed)
        );

        let (result_tx, result_rx) = oneshot::channel();
        let (claimed_tx, claimed_rx) = oneshot::channel();

        self.pending_tasks.lock().await.push_back(PendingTask {
            dispatch: TaskDispatch {
                task_id: task_id.clone(),
                task: task.clone(),
                inputs: inputs.to_vec(),
                execution_metadata,
            },
            result_tx,
            claimed_tx,
            timeout,
        });

        self.task_available.notify_one();

        if claimed_rx.await.is_err() {
            self.remove_pending_task(&task_id).await;
            return Err(anyhow!("task {task_id} was cancelled before claim"));
        }

        match result_rx.await {
            Ok(result) => Ok(result.into()),
            Err(_) => {
                self.remove_pending_task(&task_id).await;
                Err(anyhow!("task {task_id} was cancelled before completion"))
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
                    self.in_flight_tasks.lock().await.insert(
                        task_id.clone(),
                        InFlightTask {
                            worker_id: worker_id.to_string(),
                            claimed_at: Instant::now(),
                            timeout: task.timeout,
                        },
                    );
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
        self.in_flight_tasks
            .lock()
            .await
            .get(task_id)
            .map(|task| task.worker_id.clone())
    }

    fn spawn_timeout_monitor(&self, interval: Duration) {
        let pool = self.clone();
        tokio::spawn(async move {
            let mut ticker = time::interval(interval);
            loop {
                ticker.tick().await;
                pool.complete_timed_out_tasks().await;
            }
        });
    }

    async fn complete_timed_out_tasks(&self) {
        let now = Instant::now();
        let timed_out_task_ids = {
            let in_flight_tasks = self.in_flight_tasks.lock().await;
            in_flight_tasks
                .iter()
                .filter(|(_, task)| now.duration_since(task.claimed_at) >= task.timeout)
                .map(|(task_id, _)| task_id.clone())
                .collect::<Vec<_>>()
        };

        for task_id in timed_out_task_ids {
            self.complete_task_timeout(&task_id).await;
        }
    }

    async fn complete_task_timeout(&self, task_id: &str) {
        let Some(in_flight_task) = self.in_flight_tasks.lock().await.remove(task_id) else {
            return;
        };

        let waiter = self.result_waiters.lock().await.remove(task_id);
        self.mark_worker_idle_if_current(&in_flight_task.worker_id, task_id)
            .await;

        if let Some(waiter) = waiter {
            let _ = waiter.send(WorkerExecutionResult::Failure {
                reason: format!(
                    "task {task_id} timed out after {:?}",
                    in_flight_task.timeout
                ),
            });
        } else {
            warn!(worker_id = %in_flight_task.worker_id, task_id = %task_id, "timed out task had no result waiter");
        }
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
                    verifier: None,
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
                    verifier: None,
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
    async fn claimed_task_times_out_through_result_waiter() {
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

        let claimed = pool
            .claim_task("worker-1", Duration::from_secs(5))
            .await
            .unwrap()
            .unwrap();

        time::sleep(Duration::from_millis(20)).await;
        pool.complete_timed_out_tasks().await;

        assert_eq!(
            unwrap_failure(execution.await.unwrap().unwrap()),
            format!("task {} timed out after 10ms", claimed.task_id)
        );
        assert_eq!(pool.worker_for_task(&claimed.task_id).await, None);
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
            kind: TaskTypeDef::Function(crate::core::models::FunctionTaskDef::Inline {
                dependencies: vec![],
                code: "return 1".to_string(),
            }),
            verifier: None,
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

    fn unwrap_failure(result: ExecutionResult) -> String {
        match result {
            ExecutionResult::Failure(reason) => reason,
            other => panic!("expected failure, got {other:?}"),
        }
    }
}
