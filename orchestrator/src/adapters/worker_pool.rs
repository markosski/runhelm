use crate::core::models::{ExecutionMetadata, TaskDef, TaskInstance, WorkspaceKey};
use crate::core::workflow::models::{
    DispatchLease, TaskDispatchConstraints, WorkerHeartbeatState, WorkerHostId, WorkerId,
    WorkerIdentity,
};
use crate::ports::executor::ExecutionResult;
use anyhow::{anyhow, bail};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, Notify, RwLock, oneshot};
use tokio::task::JoinHandle;
use tokio::time;
use tracing::{debug, warn};

const TASK_TIMEOUT_MONITOR_INTERVAL: Duration = Duration::from_millis(100);
const HEARTBEAT_MONITOR_INTERVAL: Duration = Duration::from_millis(100);
const DEFAULT_WORKER_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const DEFAULT_MISSED_HEARTBEAT_THRESHOLD: u32 = 3;
static NEXT_WORKER_POOL_NAMESPACE_ID: AtomicU64 = AtomicU64::new(0);

pub fn workspace_key_for_task(workflow_inst_id: &str, task: &TaskDef) -> WorkspaceKey {
    match &task.workspace {
        Some(workspace) => WorkspaceKey::Group {
            workflow_inst_id: workflow_inst_id.to_string(),
            group_name: workspace.group_name.clone(),
        },
        None => WorkspaceKey::Task {
            workflow_inst_id: workflow_inst_id.to_string(),
            task_id: task.id.clone(),
        },
    }
}

pub fn workspace_path_suffix(key: &WorkspaceKey) -> PathBuf {
    match key {
        WorkspaceKey::Task {
            workflow_inst_id,
            task_id,
        } => PathBuf::from(workflow_inst_id).join(format!("taskid-{}", task_id)),
        WorkspaceKey::Group {
            workflow_inst_id,
            group_name,
        } => PathBuf::from(workflow_inst_id).join(format!("taskgroup-{}", group_name)),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerRegistration {
    pub worker_id: String,
    pub host_id: WorkerHostId,
}

impl WorkerRegistration {
    fn into_identity(self) -> WorkerIdentity {
        WorkerIdentity {
            worker_id: WorkerId::new(self.worker_id),
            host_id: self.host_id,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerHeartbeatPolicy {
    pub heartbeat_interval_ms: u64,
    pub missed_heartbeat_threshold: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDispatch {
    pub workflow_inst_id: String,
    pub task_id: String,
    pub task: TaskDef,
    pub workspace_path_suffix: PathBuf,
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
    RegistrationAck {
        worker_id: String,
        heartbeat_interval_ms: u64,
    },
    NoTask,
    TaskDispatch(TaskDispatch),
}

#[derive(Debug)]
struct WorkerState {
    identity: WorkerIdentity,
    heartbeat: WorkerHeartbeatState,
    current_task_id: Option<String>,
}

#[derive(Debug)]
struct PendingTask {
    dispatch: TaskDispatch,
    constraints: TaskDispatchConstraints,
    result_tx: oneshot::Sender<WorkerExecutionResult>,
    claimed_tx: oneshot::Sender<()>,
    timeout: Duration,
}

/// Manages queues and in-flight worker tasks.
/// Tasks are claimed by workers in FIFO order, and execution timeout tracking
/// starts when a worker claims a task.
#[derive(Debug, Clone)]
pub struct WorkerPool {
    // Registered workers and their current state
    workers: Arc<RwLock<HashMap<String, WorkerState>>>,
    // Queue of pending tasks waiting to be claimed by workers
    pending_tasks: Arc<Mutex<VecDeque<PendingTask>>>,
    // Map of in-flight task IDs to their active dispatch lease
    in_flight_tasks: Arc<Mutex<HashMap<String, DispatchLease>>>,
    // Map of task IDs to the oneshot sender waiting for their result
    result_waiters: Arc<Mutex<HashMap<String, oneshot::Sender<WorkerExecutionResult>>>>,
    // Process-local namespace so dispatch IDs from a fresh WorkerPool do not
    // collide with abandoned pre-restart worker results.
    dispatch_namespace: Arc<String>,
    // Atomic counter for generating unique task dispatch IDs
    next_dispatch_id: Arc<AtomicU64>,
    // Notify workers when a new task is available for claiming
    task_available: Arc<Notify>,
    heartbeat_interval: Duration,
    missed_heartbeat_threshold: u32,
}

impl Default for WorkerPool {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkerPool {
    pub fn new() -> Self {
        Self::new_with_heartbeat_config(
            DEFAULT_WORKER_HEARTBEAT_INTERVAL,
            DEFAULT_MISSED_HEARTBEAT_THRESHOLD,
        )
    }

    fn new_with_heartbeat_config(
        heartbeat_interval: Duration,
        missed_heartbeat_threshold: u32,
    ) -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            pending_tasks: Arc::new(Mutex::new(VecDeque::new())),
            in_flight_tasks: Arc::new(Mutex::new(HashMap::new())),
            result_waiters: Arc::new(Mutex::new(HashMap::new())),
            dispatch_namespace: Arc::new(new_dispatch_namespace()),
            next_dispatch_id: Arc::new(AtomicU64::new(0)),
            task_available: Arc::new(Notify::new()),
            heartbeat_interval,
            missed_heartbeat_threshold,
        }
    }

    pub async fn register_worker(&self, registration: WorkerRegistration) {
        self.tick_worker_heartbeat(registration).await;
    }

    /// Joins or renews a worker's in-memory liveness registration.
    ///
    /// Heartbeats are the worker pool's source of truth for which worker
    /// processes are currently available on each durable host identity.
    pub async fn tick_worker_heartbeat(&self, registration: WorkerRegistration) {
        let identity = registration.into_identity();
        let worker_id = identity.worker_id.0.clone();
        let host_id = identity.host_id.0.clone();
        let now = epoch_ms();
        let heartbeat = self.heartbeat_state(identity.clone(), now);
        let mut workers = self.workers.write().await;
        match workers.get_mut(&worker_id) {
            Some(worker) => {
                worker.identity = identity;
                worker.heartbeat = heartbeat;
            }
            None => {
                workers.insert(
                    worker_id.clone(),
                    WorkerState {
                        identity,
                        heartbeat,
                        current_task_id: None,
                    },
                );
            }
        }

        debug!(%worker_id, %host_id, "worker heartbeat joined or renewed registration");
    }

    pub fn heartbeat_policy(&self) -> WorkerHeartbeatPolicy {
        WorkerHeartbeatPolicy {
            heartbeat_interval_ms: self.heartbeat_interval.as_millis() as u64,
            missed_heartbeat_threshold: self.missed_heartbeat_threshold,
        }
    }

    pub async fn select_eligible_host(&self) -> Option<WorkerHostId> {
        self.update_worker_liveness().await;
        let workers = self.workers.read().await;
        let mut host_ids = workers
            .values()
            .filter(|worker| !worker.heartbeat.missed_heartbeat)
            .map(|worker| worker.identity.host_id.clone())
            .collect::<Vec<_>>();
        host_ids.sort_by(|left, right| left.0.cmp(&right.0));
        host_ids.dedup();
        host_ids.into_iter().next()
    }

    /// Enqueues a task and waits until it reaches a terminal worker result.
    /// Execution timeout is tracked separately after the task is claimed.
    pub async fn enqueue_task(
        &self,
        workflow_inst_id: &str,
        task: &TaskDef,
        inputs: &[serde_json::Value],
        timeout: Duration,
        execution_metadata: ExecutionMetadata,
        constraints: TaskDispatchConstraints,
    ) -> anyhow::Result<ExecutionResult> {
        let task_id = format!(
            "{}-{}-{}",
            task.id,
            self.dispatch_namespace,
            self.next_dispatch_id.fetch_add(1, Ordering::Relaxed)
        );

        let (result_tx, result_rx) = oneshot::channel();
        let (claimed_tx, claimed_rx) = oneshot::channel();

        let workspace_key = workspace_key_for_task(workflow_inst_id, task);
        let workspace_path_suffix = workspace_path_suffix(&workspace_key);

        self.pending_tasks.lock().await.push_back(PendingTask {
            dispatch: TaskDispatch {
                workflow_inst_id: workflow_inst_id.to_string(),
                task_id: task_id.clone(),
                task: task.clone(),
                workspace_path_suffix,
                inputs: inputs.to_vec(),
                execution_metadata,
            },
            constraints,
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
        let claim = async {
            loop {
                let notified = self.task_available.notified();
                self.update_worker_liveness().await;
                let worker = self.claiming_worker(worker_id).await?;

                let task = {
                    let mut in_flight_tasks = self.in_flight_tasks.lock().await;
                    if Self::is_worker_in_flight(&in_flight_tasks, worker_id) {
                        bail!("Worker has active lease")
                    }

                    let mut pending_tasks = self.pending_tasks.lock().await;
                    let Some(index) = pending_tasks.iter().position(|task| {
                        task.constraints.matches_worker(&worker)
                            && !Self::is_workflow_instance_task_in_flight(&in_flight_tasks, task)
                    }) else {
                        drop(pending_tasks);
                        drop(in_flight_tasks);

                        // Block until notified
                        notified.await;
                        continue;
                    };
                    let pending_task = pending_tasks.remove(index).unwrap();

                    let claimed_at_epoch_ms = epoch_ms();
                    let expires_at_epoch_ms =
                        claimed_at_epoch_ms + pending_task.timeout.as_millis() as u64;
                    in_flight_tasks.insert(
                        pending_task.dispatch.task_id.clone(),
                        DispatchLease {
                            dispatch_id: pending_task.dispatch.task_id.clone(),
                            workflow_instance_id: pending_task.dispatch.workflow_inst_id.clone(),
                            task_attempt_id: TaskInstance::make_task_attempt_id(
                                &pending_task.dispatch.task.id,
                                pending_task.dispatch.execution_metadata.generation_index,
                            ),
                            worker_id: worker.worker_id.clone(),
                            host_id: worker.host_id.clone(),
                            claimed_at_epoch_ms,
                            expires_at_epoch_ms,
                        },
                    );
                    pending_task
                };

                let task_id = task.dispatch.task_id.clone();
                let pinned_host_id = task.constraints.pinned_host_id.clone();
                debug!(
                    %worker_id,
                    task_id = %task_id,
                    workflow_inst_id = %task.dispatch.workflow_inst_id,
                    pinned_host_id = ?pinned_host_id,
                    "worker claimed pending task"
                );

                self.result_waiters
                    .lock()
                    .await
                    .insert(task_id.clone(), task.result_tx);

                self.mark_worker_running(worker_id, &task_id).await;
                let _ = task.claimed_tx.send(());
                return Ok(task.dispatch);
            }
        };

        match time::timeout(timeout, claim).await {
            Ok(result) => result.map(Some),
            Err(_) => Ok(None),
        }
    }

    // Ensure that given pending task does not have matching in-flight tasks running for same workflow instance
    fn is_workflow_instance_task_in_flight(
        in_flight_tasks: &HashMap<String, DispatchLease>,
        pending_task: &PendingTask,
    ) -> bool {
        in_flight_tasks
            .iter()
            .any(|(_, lease)| lease.workflow_instance_id == pending_task.dispatch.workflow_inst_id)
    }

    fn is_worker_in_flight(leases: &HashMap<String, DispatchLease>, worker_id: &str) -> bool {
        leases.values().any(|lease| lease.worker_id.0 == worker_id)
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

    /// Returns the worker that currently owns an active dispatch lease for the
    /// given worker-facing dispatch ID, if that dispatch is still in flight.
    pub async fn worker_for_active_dispatch(&self, task_id: &str) -> Option<String> {
        self.in_flight_tasks
            .lock()
            .await
            .get(task_id)
            .map(|lease| lease.worker_id.0.clone())
    }

    async fn update_worker_liveness(&self) {
        let now = epoch_ms();
        let mut workers = self.workers.write().await;
        workers.retain(|worker_id, worker| {
            if worker.heartbeat.deregister_after_epoch_ms <= now {
                debug!(%worker_id, "deregistering worker after missed heartbeat threshold");
                return false;
            }

            worker.heartbeat.missed_heartbeat = worker.heartbeat.next_heartbeat_due_epoch_ms <= now;
            true
        });
    }

    async fn complete_timed_out_tasks(&self) {
        let now = epoch_ms();
        let timed_out_task_ids = {
            let in_flight_tasks = self.in_flight_tasks.lock().await;
            in_flight_tasks
                .iter()
                .filter(|(_, lease)| lease.is_expired_at(now))
                .map(|(task_id, _)| task_id.clone())
                .collect::<Vec<_>>()
        };

        for task_id in timed_out_task_ids {
            self.complete_task_timeout(&task_id).await;
        }
    }

    async fn complete_task_timeout(&self, task_id: &str) {
        let Some(lease) = self.in_flight_tasks.lock().await.remove(task_id) else {
            return;
        };

        let waiter = self.result_waiters.lock().await.remove(task_id);
        self.mark_worker_idle_if_current(&lease.worker_id.0, task_id)
            .await;

        if let Some(waiter) = waiter {
            let timeout =
                Duration::from_millis(lease.expires_at_epoch_ms - lease.claimed_at_epoch_ms);
            let _ = waiter.send(WorkerExecutionResult::Failure {
                reason: format!("task {task_id} timed out after {:?}", timeout),
            });
        } else {
            warn!(worker_id = %lease.worker_id.0, task_id = %task_id, "timed out task had no result waiter");
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

    async fn claiming_worker(&self, worker_id: &str) -> anyhow::Result<WorkerIdentity> {
        let workers = self.workers.read().await;
        let Some(worker) = workers.get(worker_id) else {
            anyhow::bail!("worker {worker_id} is not registered");
        };

        if worker.heartbeat.missed_heartbeat {
            anyhow::bail!("worker {worker_id} missed heartbeat");
        }

        Ok(worker.identity.clone())
    }

    fn heartbeat_state(&self, identity: WorkerIdentity, now_epoch_ms: u64) -> WorkerHeartbeatState {
        let interval_ms = self.heartbeat_interval.as_millis() as u64;
        let threshold = u64::from(self.missed_heartbeat_threshold.max(1));
        WorkerHeartbeatState {
            identity,
            last_heartbeat_at_epoch_ms: now_epoch_ms,
            next_heartbeat_due_epoch_ms: now_epoch_ms + interval_ms,
            deregister_after_epoch_ms: now_epoch_ms + (interval_ms * threshold),
            missed_heartbeat: false,
        }
    }
}

fn epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn new_dispatch_namespace() -> String {
    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let sequence = NEXT_WORKER_POOL_NAMESPACE_ID.fetch_add(1, Ordering::Relaxed);
    format!("{}-{now_nanos:x}-{sequence:x}", std::process::id())
}

pub fn start_task_timeout_monitor(worker_pool: WorkerPool) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = time::interval(TASK_TIMEOUT_MONITOR_INTERVAL);
        loop {
            ticker.tick().await;
            worker_pool.complete_timed_out_tasks().await;
        }
    })
}

pub fn start_worker_heartbeat_monitor(worker_pool: WorkerPool) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = time::interval(HEARTBEAT_MONITOR_INTERVAL);
        loop {
            ticker.tick().await;
            worker_pool.update_worker_liveness().await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{TaskDef, TaskTypeDef, Workspace};
    use serde_json::json;

    fn test_registration(worker_id: &str) -> WorkerRegistration {
        test_registration_for_host(worker_id, "test-host")
    }

    fn test_registration_for_host(worker_id: &str, host_id: &str) -> WorkerRegistration {
        WorkerRegistration {
            worker_id: worker_id.to_string(),
            host_id: WorkerHostId::new(host_id),
        }
    }

    fn heartbeat_test_pool() -> WorkerPool {
        WorkerPool::new_with_heartbeat_config(Duration::from_millis(10), 3)
    }

    async fn wait_for_pending_tasks(pool: &WorkerPool, expected_count: usize) {
        for _ in 0..10 {
            if pool.pending_tasks.lock().await.len() == expected_count {
                return;
            }
            time::sleep(Duration::from_millis(1)).await;
        }

        assert_eq!(pool.pending_tasks.lock().await.len(), expected_count);
    }

    #[tokio::test]
    async fn registration_preserves_worker_identity_separately_from_host_identity() {
        let pool = WorkerPool::new();
        pool.register_worker(test_registration("worker-1")).await;
        pool.register_worker(test_registration("worker-2")).await;

        let workers = pool.workers.read().await;
        let worker_1 = workers.get("worker-1").unwrap();
        let worker_2 = workers.get("worker-2").unwrap();

        assert_eq!(worker_1.identity.worker_id, WorkerId::new("worker-1"));
        assert_eq!(worker_2.identity.worker_id, WorkerId::new("worker-2"));
        assert_eq!(worker_1.identity.host_id, WorkerHostId::new("test-host"));
        assert_eq!(worker_2.identity.host_id, WorkerHostId::new("test-host"));
    }

    #[tokio::test]
    async fn heartbeat_joins_worker_registration() {
        let pool = heartbeat_test_pool();
        pool.tick_worker_heartbeat(test_registration("worker-1"))
            .await;

        let workers = pool.workers.read().await;
        let worker = workers.get("worker-1").unwrap();

        assert_eq!(worker.identity.worker_id, WorkerId::new("worker-1"));
        assert_eq!(worker.identity.host_id, WorkerHostId::new("test-host"));
        assert!(!worker.heartbeat.missed_heartbeat);
    }

    #[tokio::test]
    async fn missed_heartbeat_marks_worker_suspicious_and_prevents_claims() {
        let pool = heartbeat_test_pool();
        pool.tick_worker_heartbeat(test_registration("worker-1"))
            .await;

        time::sleep(Duration::from_millis(15)).await;
        pool.update_worker_liveness().await;

        {
            let workers = pool.workers.read().await;
            assert!(workers.get("worker-1").unwrap().heartbeat.missed_heartbeat);
        }

        let error = pool
            .claim_task("worker-1", Duration::from_millis(1))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("missed heartbeat"));
    }

    #[tokio::test]
    async fn heartbeat_renewal_clears_missed_heartbeat() {
        let pool = heartbeat_test_pool();
        pool.tick_worker_heartbeat(test_registration("worker-1"))
            .await;

        time::sleep(Duration::from_millis(15)).await;
        pool.update_worker_liveness().await;
        assert!(
            pool.workers
                .read()
                .await
                .get("worker-1")
                .unwrap()
                .heartbeat
                .missed_heartbeat
        );

        pool.tick_worker_heartbeat(test_registration("worker-1"))
            .await;

        assert!(
            !pool
                .workers
                .read()
                .await
                .get("worker-1")
                .unwrap()
                .heartbeat
                .missed_heartbeat
        );
    }

    #[tokio::test]
    async fn missed_heartbeat_threshold_deregisters_worker() {
        let pool = heartbeat_test_pool();
        pool.tick_worker_heartbeat(test_registration("worker-1"))
            .await;

        time::sleep(Duration::from_millis(35)).await;
        pool.update_worker_liveness().await;

        assert!(!pool.workers.read().await.contains_key("worker-1"));
    }

    #[tokio::test]
    async fn deregistered_worker_rejoins_by_heartbeat() {
        let pool = heartbeat_test_pool();
        pool.tick_worker_heartbeat(test_registration_for_host("worker-1", "host-a"))
            .await;

        time::sleep(Duration::from_millis(35)).await;
        pool.update_worker_liveness().await;
        assert!(!pool.workers.read().await.contains_key("worker-1"));

        pool.tick_worker_heartbeat(test_registration_for_host("worker-1", "host-a"))
            .await;

        let workers = pool.workers.read().await;
        let worker = workers.get("worker-1").unwrap();
        assert_eq!(worker.identity.worker_id, WorkerId::new("worker-1"));
        assert_eq!(worker.identity.host_id, WorkerHostId::new("host-a"));
        assert!(!worker.heartbeat.missed_heartbeat);
    }

    #[tokio::test]
    async fn select_eligible_host_returns_registered_non_suspicious_host() {
        let pool = WorkerPool::new();
        pool.register_worker(test_registration_for_host("worker-z", "host-z"))
            .await;
        pool.register_worker(test_registration_for_host("worker-a", "host-a"))
            .await;

        assert_eq!(
            pool.select_eligible_host().await,
            Some(WorkerHostId::new("host-a"))
        );
    }

    #[tokio::test]
    async fn select_eligible_host_ignores_missed_heartbeat_workers() {
        let pool = heartbeat_test_pool();
        pool.register_worker(test_registration_for_host("worker-1", "host-a"))
            .await;

        time::sleep(Duration::from_millis(15)).await;

        assert_eq!(pool.select_eligible_host().await, None);
    }

    #[tokio::test]
    async fn worker_claims_queued_task_and_completes_result() {
        let pool = WorkerPool::new();
        pool.register_worker(test_registration("worker-1")).await;

        let task = test_task("task-1");
        let execution_pool = pool.clone();
        let execution = tokio::spawn(async move {
            execution_pool
                .enqueue_task(
                    "123",
                    &task,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints::default(),
                )
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
    async fn worker_with_active_lease_cannot_claim_another_task() {
        let pool = WorkerPool::new();
        pool.register_worker(test_registration("worker-1")).await;

        let first_task = test_task("task-1");
        let first_pool = pool.clone();
        let first_execution = tokio::spawn(async move {
            first_pool
                .enqueue_task(
                    "workflow-1",
                    &first_task,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints::default(),
                )
                .await
        });

        let first_claim = pool
            .claim_task("worker-1", Duration::from_secs(5))
            .await
            .unwrap()
            .unwrap();

        let second_task = test_task("task-2");

        // Clonig again so that when clone is moved we can still operate on the original pool, internals of the pools are behind Arcs
        let second_pool = pool.clone();
        let second_execution = tokio::spawn(async move {
            second_pool
                .enqueue_task(
                    "workflow-2",
                    &second_task,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints::default(),
                )
                .await
        });

        wait_for_pending_tasks(&pool, 1).await;

        let error = pool
            .claim_task("worker-1", Duration::from_millis(10))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("active lease"));

        pool.complete_task_result(
            "worker-1",
            TaskResult {
                task_id: first_claim.task_id,
                result: WorkerExecutionResult::Success {
                    output: json!({"task": "first"}),
                },
            },
        )
        .await
        .unwrap();

        let second_claim = pool
            .claim_task("worker-1", Duration::from_millis(10))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(second_claim.workflow_inst_id, "workflow-2");
        assert_eq!(second_claim.task.id, "task-2");

        pool.complete_task_result(
            "worker-1",
            TaskResult {
                task_id: second_claim.task_id,
                result: WorkerExecutionResult::Success {
                    output: json!({"task": "second"}),
                },
            },
        )
        .await
        .unwrap();

        assert_eq!(
            unwrap_success(first_execution.await.unwrap().unwrap()),
            json!({"task": "first"})
        );
        assert_eq!(
            unwrap_success(second_execution.await.unwrap().unwrap()),
            json!({"task": "second"})
        );
    }

    #[tokio::test]
    async fn queued_task_does_not_time_out_before_claim() {
        let pool = WorkerPool::new();
        pool.register_worker(test_registration("worker-1")).await;

        let task = test_task("task-1");
        let execution_pool = pool.clone();
        let execution = tokio::spawn(async move {
            execution_pool
                .enqueue_task(
                    "123",
                    &task,
                    &[],
                    Duration::from_millis(10),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints::default(),
                )
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
        pool.register_worker(test_registration("worker-1")).await;

        let task = test_task("task-1");
        let execution_pool = pool.clone();
        let execution = tokio::spawn(async move {
            execution_pool
                .enqueue_task(
                    "123",
                    &task,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints::default(),
                )
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
    async fn claimed_task_dispatch_includes_logical_workspace_metadata() {
        let pool = WorkerPool::new();
        pool.register_worker(test_registration("worker-1")).await;

        let task = test_task("task-1");
        let execution_pool = pool.clone();
        let execution = tokio::spawn(async move {
            execution_pool
                .enqueue_task(
                    "123",
                    &task,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints::default(),
                )
                .await
        });

        let claimed = pool
            .claim_task("worker-1", Duration::from_millis(10))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            serde_json::to_value(&claimed).unwrap()["workspace_path_suffix"],
            json!("123/taskid-task-1")
        );

        execution.abort();
    }

    #[tokio::test]
    async fn shared_workspace_tasks_reuse_suffix_and_workflow_pin() {
        let pool = WorkerPool::new();
        pool.register_worker(test_registration_for_host("worker-a", "host-a"))
            .await;
        pool.register_worker(test_registration_for_host("worker-b", "host-b"))
            .await;

        let mut first_task = test_task("first");
        first_task.workspace = Some(Workspace {
            group_name: "repo".to_string(),
        });
        let first_pool = pool.clone();
        let first_execution = tokio::spawn(async move {
            first_pool
                .enqueue_task(
                    "workflow-1",
                    &first_task,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints {
                        pinned_host_id: Some(WorkerHostId::new("host-a")),
                    },
                )
                .await
        });

        let mut second_task = test_task("second");
        second_task.workspace = Some(Workspace {
            group_name: "repo".to_string(),
        });
        let second_pool = pool.clone();
        let second_execution = tokio::spawn(async move {
            second_pool
                .enqueue_task(
                    "workflow-1",
                    &second_task,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints {
                        pinned_host_id: Some(WorkerHostId::new("host-a")),
                    },
                )
                .await
        });

        wait_for_pending_tasks(&pool, 2).await;

        {
            let pending_tasks = pool.pending_tasks.lock().await;
            assert!(pending_tasks.iter().all(|task| {
                task.dispatch.workspace_path_suffix == PathBuf::from("workflow-1/taskgroup-repo")
                    && task.constraints.pinned_host_id == Some(WorkerHostId::new("host-a"))
            }));
        }

        let mismatched_claim = pool
            .claim_task("worker-b", Duration::from_millis(10))
            .await
            .unwrap();
        assert!(mismatched_claim.is_none());

        let first_claim = pool
            .claim_task("worker-a", Duration::from_millis(10))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            first_claim.workspace_path_suffix,
            PathBuf::from("workflow-1/taskgroup-repo")
        );

        pool.complete_task_result(
            "worker-a",
            TaskResult {
                task_id: first_claim.task_id,
                result: WorkerExecutionResult::Success {
                    output: json!({"task": "first"}),
                },
            },
        )
        .await
        .unwrap();

        let second_claim = pool
            .claim_task("worker-a", Duration::from_millis(10))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            second_claim.workspace_path_suffix,
            PathBuf::from("workflow-1/taskgroup-repo")
        );

        first_execution.abort();
        second_execution.abort();
    }

    #[tokio::test]
    async fn pending_task_preserves_workflow_pin_constraint() {
        let pool = WorkerPool::new();
        let task = test_task("task-1");
        let execution_pool = pool.clone();
        let execution = tokio::spawn(async move {
            execution_pool
                .enqueue_task(
                    "workflow-1",
                    &task,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints {
                        pinned_host_id: Some(WorkerHostId::new("host-a")),
                    },
                )
                .await
        });

        wait_for_pending_tasks(&pool, 1).await;

        let pending_tasks = pool.pending_tasks.lock().await;
        let pending = pending_tasks.front().unwrap();
        assert_eq!(pending.dispatch.workflow_inst_id, "workflow-1");
        assert_eq!(
            pending.constraints.pinned_host_id,
            Some(WorkerHostId::new("host-a"))
        );

        execution.abort();
    }

    #[tokio::test]
    async fn matching_host_claims_pinned_task() {
        let pool = WorkerPool::new();
        pool.register_worker(test_registration_for_host("worker-1", "host-a"))
            .await;

        let task = test_task("task-1");
        let execution_pool = pool.clone();
        let execution = tokio::spawn(async move {
            execution_pool
                .enqueue_task(
                    "workflow-1",
                    &task,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints {
                        pinned_host_id: Some(WorkerHostId::new("host-a")),
                    },
                )
                .await
        });

        let claimed = pool
            .claim_task("worker-1", Duration::from_millis(10))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(claimed.workflow_inst_id, "workflow-1");
        execution.abort();
    }

    #[tokio::test]
    async fn multiple_workers_sharing_host_can_claim_pinned_workflows() {
        let pool = WorkerPool::new();
        pool.register_worker(test_registration_for_host("worker-1", "host-a"))
            .await;
        pool.register_worker(test_registration_for_host("worker-2", "host-a"))
            .await;

        let task_a = test_task("task-a");
        let first_pool = pool.clone();
        let first_execution = tokio::spawn(async move {
            first_pool
                .enqueue_task(
                    "workflow-a",
                    &task_a,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints {
                        pinned_host_id: Some(WorkerHostId::new("host-a")),
                    },
                )
                .await
        });

        let task_b = test_task("task-b");
        let second_pool = pool.clone();
        let second_execution = tokio::spawn(async move {
            second_pool
                .enqueue_task(
                    "workflow-b",
                    &task_b,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints {
                        pinned_host_id: Some(WorkerHostId::new("host-a")),
                    },
                )
                .await
        });

        wait_for_pending_tasks(&pool, 2).await;

        let first_claim = pool
            .claim_task("worker-1", Duration::from_millis(10))
            .await
            .unwrap()
            .unwrap();
        let second_claim = pool
            .claim_task("worker-2", Duration::from_millis(10))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(first_claim.workflow_inst_id, "workflow-a");
        assert_eq!(second_claim.workflow_inst_id, "workflow-b");
        first_execution.abort();
        second_execution.abort();
    }

    #[tokio::test]
    async fn claimed_task_records_dispatch_lease_metadata() {
        let pool = WorkerPool::new();
        pool.register_worker(test_registration_for_host("worker-1", "host-a"))
            .await;

        let task = test_task("task-1");
        let execution_pool = pool.clone();
        let execution = tokio::spawn(async move {
            execution_pool
                .enqueue_task(
                    "workflow-1",
                    &task,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata {
                        generation_index: 3,
                        loop_context: None,
                    },
                    TaskDispatchConstraints {
                        pinned_host_id: Some(WorkerHostId::new("host-a")),
                    },
                )
                .await
        });

        let before_claim_epoch_ms = epoch_ms();
        let claimed = pool
            .claim_task("worker-1", Duration::from_millis(10))
            .await
            .unwrap()
            .unwrap();
        let after_claim_epoch_ms = epoch_ms();

        let in_flight_tasks = pool.in_flight_tasks.lock().await;
        let lease = in_flight_tasks.get(&claimed.task_id).unwrap();

        assert_eq!(lease.dispatch_id, claimed.task_id);
        assert_eq!(lease.workflow_instance_id, "workflow-1");
        assert_eq!(lease.task_attempt_id, "task-1[3]");
        assert_eq!(lease.worker_id, WorkerId::new("worker-1"));
        assert_eq!(lease.host_id, WorkerHostId::new("host-a"));
        assert!(lease.claimed_at_epoch_ms >= before_claim_epoch_ms);
        assert!(lease.claimed_at_epoch_ms <= after_claim_epoch_ms);
        assert_eq!(lease.expires_at_epoch_ms - lease.claimed_at_epoch_ms, 5_000);

        execution.abort();
    }

    #[tokio::test]
    async fn stale_pre_restart_dispatch_result_does_not_complete_recovered_dispatch() {
        // Simulate the pre-restart worker pool claiming a dispatch whose worker
        // may keep running after the orchestrator process exits.
        let old_pool = WorkerPool::new();
        old_pool
            .register_worker(test_registration_for_host("old-worker", "host-a"))
            .await;

        let task = test_task("task-1");
        let old_execution_pool = old_pool.clone();
        let old_task = task.clone();
        let old_execution = tokio::spawn(async move {
            old_execution_pool
                .enqueue_task(
                    "workflow-1",
                    &old_task,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints {
                        pinned_host_id: Some(WorkerHostId::new("host-a")),
                    },
                )
                .await
        });

        // Capture the old dispatch ID as the stale completion key a pre-restart
        // worker would later post back to the restarted orchestrator.
        let old_claim = old_pool
            .claim_task("old-worker", Duration::from_millis(10))
            .await
            .unwrap()
            .unwrap();
        let stale_task_id = old_claim.task_id;

        // Stop the old workflow-side waiter while keeping stale_task_id as the
        // result key an already-running pre-restart worker would still hold.
        old_execution.abort();

        // Simulate a restarted orchestrator with a fresh WorkerPool recovering
        // the same logical workflow task on the same pinned host.
        let recovered_pool = WorkerPool::new();
        recovered_pool
            .register_worker(test_registration_for_host("new-worker", "host-a"))
            .await;

        let recovered_execution_pool = recovered_pool.clone();
        let recovered_execution = tokio::spawn(async move {
            recovered_execution_pool
                .enqueue_task(
                    "workflow-1",
                    &task,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints {
                        pinned_host_id: Some(WorkerHostId::new("host-a")),
                    },
                )
                .await
        });

        let recovered_claim = recovered_pool
            .claim_task("new-worker", Duration::from_millis(10))
            .await
            .unwrap()
            .unwrap();

        // The fresh pool's dispatch namespace must keep the recovered dispatch
        // distinct from the stale pre-restart dispatch ID.
        assert_ne!(stale_task_id, recovered_claim.task_id);

        // A stale completion should be treated as late/untracked and must not
        // wake the recovered dispatch waiter.
        recovered_pool
            .complete_task_result(
                "old-worker",
                TaskResult {
                    task_id: stale_task_id,
                    result: WorkerExecutionResult::Success {
                        output: json!({"source": "stale"}),
                    },
                },
            )
            .await
            .unwrap();

        assert_eq!(
            recovered_pool
                .worker_for_active_dispatch(&recovered_claim.task_id)
                .await,
            Some("new-worker".to_string())
        );

        // The recovered dispatch should still complete only when its own
        // current dispatch ID reports a result.
        recovered_pool
            .complete_task_result(
                "new-worker",
                TaskResult {
                    task_id: recovered_claim.task_id,
                    result: WorkerExecutionResult::Success {
                        output: json!({"source": "recovered"}),
                    },
                },
            )
            .await
            .unwrap();

        assert_eq!(
            unwrap_success(recovered_execution.await.unwrap().unwrap()),
            json!({"source": "recovered"})
        );
    }

    #[tokio::test]
    async fn mismatched_host_does_not_claim_pinned_task() {
        let pool = WorkerPool::new();
        pool.register_worker(test_registration_for_host("worker-1", "host-b"))
            .await;

        let task = test_task("task-1");
        let execution_pool = pool.clone();
        let execution = tokio::spawn(async move {
            execution_pool
                .enqueue_task(
                    "workflow-1",
                    &task,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints {
                        pinned_host_id: Some(WorkerHostId::new("host-a")),
                    },
                )
                .await
        });
        wait_for_pending_tasks(&pool, 1).await;

        let claimed = pool
            .claim_task("worker-1", Duration::from_millis(10))
            .await
            .unwrap();

        assert!(claimed.is_none());
        assert!(!execution.is_finished());
        assert_eq!(pool.pending_tasks.lock().await.len(), 1);
        execution.abort();
    }

    #[tokio::test]
    async fn worker_claim_scans_past_nonmatching_pinned_tasks() {
        let pool = WorkerPool::new();
        pool.register_worker(test_registration_for_host("worker-1", "host-b"))
            .await;

        let task_a = test_task("task-a");
        let task_b = test_task("task-b");
        let first_pool = pool.clone();
        let first = tokio::spawn(async move {
            first_pool
                .enqueue_task(
                    "workflow-a",
                    &task_a,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints {
                        pinned_host_id: Some(WorkerHostId::new("host-a")),
                    },
                )
                .await
        });
        let second_pool = pool.clone();
        let second = tokio::spawn(async move {
            second_pool
                .enqueue_task(
                    "workflow-b",
                    &task_b,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints {
                        pinned_host_id: Some(WorkerHostId::new("host-b")),
                    },
                )
                .await
        });
        wait_for_pending_tasks(&pool, 2).await;

        let claimed = pool
            .claim_task("worker-1", Duration::from_millis(10))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(claimed.workflow_inst_id, "workflow-b");
        assert_eq!(pool.pending_tasks.lock().await.len(), 1);
        first.abort();
        second.abort();
    }

    #[tokio::test]
    async fn worker_claim_skips_workflow_with_active_dispatch() {
        let pool = WorkerPool::new();
        pool.register_worker(test_registration_for_host("worker-1", "host-a"))
            .await;
        pool.register_worker(test_registration_for_host("worker-2", "host-a"))
            .await;

        let active_task = test_task("active-task");
        let active_pool = pool.clone();
        let active_execution = tokio::spawn(async move {
            active_pool
                .enqueue_task(
                    "workflow-1",
                    &active_task,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints {
                        pinned_host_id: Some(WorkerHostId::new("host-a")),
                    },
                )
                .await
        });

        let active_claim = pool
            .claim_task("worker-1", Duration::from_millis(10))
            .await
            .unwrap()
            .unwrap();

        let blocked_task = test_task("blocked-task");
        let blocked_pool = pool.clone();
        let blocked_execution = tokio::spawn(async move {
            blocked_pool
                .enqueue_task(
                    "workflow-1",
                    &blocked_task,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints {
                        pinned_host_id: Some(WorkerHostId::new("host-a")),
                    },
                )
                .await
        });

        let other_task = test_task("other-task");
        let other_pool = pool.clone();
        let other_execution = tokio::spawn(async move {
            other_pool
                .enqueue_task(
                    "workflow-2",
                    &other_task,
                    &[],
                    Duration::from_secs(5),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints {
                        pinned_host_id: Some(WorkerHostId::new("host-a")),
                    },
                )
                .await
        });

        wait_for_pending_tasks(&pool, 2).await;

        let other_claim = pool
            .claim_task("worker-2", Duration::from_millis(10))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(other_claim.workflow_inst_id, "workflow-2");
        assert_eq!(pool.pending_tasks.lock().await.len(), 1);

        pool.complete_task_result(
            "worker-1",
            TaskResult {
                task_id: active_claim.task_id,
                result: WorkerExecutionResult::Success {
                    output: json!({"task": "active"}),
                },
            },
        )
        .await
        .unwrap();

        let blocked_claim = pool
            .claim_task("worker-1", Duration::from_millis(10))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(blocked_claim.workflow_inst_id, "workflow-1");
        assert_eq!(blocked_claim.task.id, "blocked-task");

        pool.complete_task_result(
            "worker-1",
            TaskResult {
                task_id: blocked_claim.task_id,
                result: WorkerExecutionResult::Success {
                    output: json!({"task": "blocked"}),
                },
            },
        )
        .await
        .unwrap();
        pool.complete_task_result(
            "worker-2",
            TaskResult {
                task_id: other_claim.task_id,
                result: WorkerExecutionResult::Success {
                    output: json!({"task": "other"}),
                },
            },
        )
        .await
        .unwrap();

        assert_eq!(
            unwrap_success(active_execution.await.unwrap().unwrap()),
            json!({"task": "active"})
        );
        assert_eq!(
            unwrap_success(blocked_execution.await.unwrap().unwrap()),
            json!({"task": "blocked"})
        );
        assert_eq!(
            unwrap_success(other_execution.await.unwrap().unwrap()),
            json!({"task": "other"})
        );
    }

    #[tokio::test]
    async fn claimed_task_times_out_through_result_waiter() {
        let pool = WorkerPool::new();
        pool.register_worker(test_registration("worker-1")).await;

        let task = test_task("task-1");
        let execution_pool = pool.clone();
        let execution = tokio::spawn(async move {
            execution_pool
                .enqueue_task(
                    "123",
                    &task,
                    &[],
                    Duration::from_millis(10),
                    ExecutionMetadata::default(),
                    TaskDispatchConstraints::default(),
                )
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
        assert_eq!(pool.worker_for_active_dispatch(&claimed.task_id).await, None);
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
            control: None,
            timeout_secs: None,
            input_schemas: vec![],
            output_schema: None,
            workspace: None,
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
