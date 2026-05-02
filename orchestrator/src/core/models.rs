use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub status: WorkflowStatus,
    // Add other fields: tasks, inputs, etc.
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkflowStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
}

pub type JsonSchema = serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskTypeDef {
    ApiCall { url: String, method: String },
    Agent { agent_id: String, prompt: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDef {
    pub id: String,
    pub kind: TaskTypeDef,
    pub input_schemas: Vec<JsonSchema>,
    pub output_schema: JsonSchema,
    pub expected_side_effects: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataBinding {
    pub target_task_id: String,
    pub target_input_index: usize,
    pub source_task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDef {
    pub id: String,
    pub tasks: Vec<TaskDef>,
    pub data_bindings: Vec<DataBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideEffectInstance {
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInstance {
    pub task_def_id: String,
    pub status: TaskStatus,
    pub input_data: Option<serde_json::Value>,
    pub output_data: Option<serde_json::Value>,
    pub recorded_side_effects: Vec<SideEffectInstance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInstance {
    pub id: String,
    pub workflow_def_id: String,
    pub status: WorkflowStatus,
    pub tasks: std::collections::HashMap<String, TaskInstance>,
}

/// A lightweight read model describing the current state of a workflow instance.
/// Intended for status queries — does not expose raw input/output data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStatusReport {
    pub instance_id: String,
    pub workflow_def_id: String,
    pub status: WorkflowStatus,
    pub tasks: Vec<TaskStatusReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusReport {
    pub task_id: String,
    pub status: TaskStatus,
    /// True when the task has produced output data.
    pub has_output: bool,
}
