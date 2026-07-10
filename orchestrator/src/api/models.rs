use crate::adapters::task_dispatcher::TaskDispatch;
use crate::core::{
    models::{TaskInputMapping, TaskSatisfactionStatus, TaskStatus, VerifierAttemptMetadata},
    workflow::events::WorkflowEventRecord,
    workflow::models::{VerifierStateStatus, WorkflowDefSummary, WorkflowInfo, WorkflowStatus},
};
use serde::{Deserialize, Serialize};

/// A lightweight read model describing the current state of a workflow instance.
/// Intended for status queries - does not expose raw input/output data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStatusReport {
    pub instance_id: String,
    pub workflow_def_id: String,
    pub status: WorkflowStatus,
    pub tasks: Vec<TaskStatusReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verifier_states: Vec<VerifierStatusReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowList {
    pub workflows: Vec<WorkflowInfo>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowDefList {
    pub workflow_defs: Vec<WorkflowDefSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEvents {
    pub workflow_instance_id: String,
    pub events: Vec<WorkflowEventRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowQueueStatus {
    pub pending: Vec<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusReport {
    pub task_attempt_id: String,
    pub task_def_id: String,
    pub status: TaskStatus,
    pub satisfaction: TaskSatisfactionStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_mapping: Vec<TaskInputMapping>,
    pub generation_index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifier_metadata: Option<VerifierAttemptMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierStatusReport {
    pub verifier_task_id: String,
    pub rerun_start_task_id: String,
    pub latest_generation: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_generation: Option<u32>,
    pub status: VerifierStateStatus,
    pub feedback_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_reason: Option<String>,
}
