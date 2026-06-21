use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::core::models::{TaskDef, TaskInstance, TaskStatus};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[allow(dead_code)]
pub struct WorkerId(pub String);

#[allow(dead_code)]
impl WorkerId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkerHostId(pub String);

impl WorkerHostId {
    #[allow(dead_code)]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct WorkerHeartbeatState {
    pub worker_id: WorkerId,
    pub host_id: WorkerHostId,
    pub last_heartbeat_at_epoch_ms: u64,
    pub expires_at_epoch_ms: u64,
}

#[allow(dead_code)]
impl WorkerHeartbeatState {
    pub fn is_expired_at(&self, now_epoch_ms: u64) -> bool {
        self.expires_at_epoch_ms <= now_epoch_ms
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkflowStatus {
    Pending,
    Running,
    Paused,
    InputNeeded,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInstance {
    pub id: String,
    pub workflow_def_id: String,
    pub status: WorkflowStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_worker_host: Option<WorkerHostId>,
    // Keyed by task_attempt_id, e.g. "task-a[2]".
    pub tasks: HashMap<String, TaskInstance>,
    #[serde(default)]
    pub verifier_states: HashMap<String, VerifierGenerationState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowInfo {
    pub id: String,
    pub workflow_def_id: String,
    pub created_at_epoch_ms: Option<u64>,
    pub modified_at_epoch_ms: u64,
    pub completed_at_epoch_ms: Option<u64>,
    pub status: WorkflowStatus,
    pub total_task_count: usize,
    pub completed_task_count: usize,
}

impl WorkflowInfo {
    pub fn from_instance_with_timestamps(
        instance: &WorkflowInstance,
        created_at_epoch_ms: Option<u64>,
        modified_at_epoch_ms: u64,
        completed_at_epoch_ms: Option<u64>,
    ) -> Self {
        Self {
            id: instance.id.clone(),
            workflow_def_id: instance.workflow_def_id.clone(),
            created_at_epoch_ms,
            modified_at_epoch_ms,
            completed_at_epoch_ms,
            status: instance.status.clone(),
            total_task_count: instance.tasks.len(),
            completed_task_count: instance
                .tasks
                .values()
                .filter(|task| task.status == TaskStatus::Completed)
                .count(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierGenerationState {
    pub verifier_task_id: String,
    pub rerun_start_task_id: String,
    pub latest_generation: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_generation: Option<u32>,
    #[serde(default)]
    pub feedback_history: Vec<VerifierFeedbackEntry>,
    pub status: VerifierStateStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierFeedbackEntry {
    pub generation_index: u32,
    pub feedback: String,
    pub verifier_output: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerifierStateStatus {
    Running,
    Accepted,
    ExhaustedAccepted,
    ExhaustedFailed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataBinding {
    pub target_task_id: String,
    pub source_task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDef {
    pub id: String,
    pub tasks: Vec<TaskDef>,
    pub data_bindings: Vec<DataBinding>,
}
