use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkerId(pub String);

impl WorkerId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkerHostId(pub String);

impl WorkerHostId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerIdentity {
    pub worker_id: WorkerId,
    pub host_id: WorkerHostId,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskDispatchConstraints {
    pub pinned_host_id: Option<WorkerHostId>,
}

impl TaskDispatchConstraints {
    pub fn matches_worker(&self, worker: &WorkerIdentity) -> bool {
        match &self.pinned_host_id {
            Some(host_id) => host_id == &worker.host_id,
            None => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct WorkerHeartbeatState {
    pub identity: WorkerIdentity,
    pub last_heartbeat_at_epoch_ms: u64,
    pub next_heartbeat_due_epoch_ms: u64,
    pub deregister_after_epoch_ms: u64,
    pub missed_heartbeat: bool,
}

#[allow(dead_code)]
impl WorkerHeartbeatState {
    pub fn is_expired_at(&self, now_epoch_ms: u64) -> bool {
        self.deregister_after_epoch_ms <= now_epoch_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct DispatchLease {
    pub dispatch_id: String,
    pub workflow_instance_id: String,
    pub task_attempt_id: String,
    pub worker_id: WorkerId,
    pub host_id: WorkerHostId,
    pub claimed_at_epoch_ms: u64,
    pub expires_at_epoch_ms: u64,
}

#[allow(dead_code)]
impl DispatchLease {
    pub fn is_expired_at(&self, now_epoch_ms: u64) -> bool {
        self.expires_at_epoch_ms <= now_epoch_ms
    }
}
