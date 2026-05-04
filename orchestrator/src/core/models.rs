use serde::{Deserialize, Serialize};
use serde_json::Number;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkflowStatus {
    Pending,
    Running,
    Paused,
    InputNeeded,
    Completed,
    Failed,
}

pub type JsonSchema = serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskTypeDef {
    ApiCall {
        url: String,
        method: String,
    },
    Agent {
        // Model name, e.g. sonnet, oput, gpt-5.5, gemini-2.5-flash, etc.
        model_id: String,
        provider_url: String,
        // Agent prompt
        prompt: String,
        // Allowed tools, [] - none, ["_all_"] - all available tools
        tools: Vec<String>,
        // Gives agent allowance to pause task to get additional information if needed
        ask: bool,
        // How many times agent should re-try when output does not match expected output_schema
        schema_failure_retry_times: Number,
    },
    Function {
        // Task will attempt to download these dependencies
        dependencies: Vec<FunctionDependency>,
        code: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDependency {
    name: String,
    version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDef {
    pub id: String,
    pub kind: TaskTypeDef,
    pub input_schemas: Vec<JsonSchema>,
    pub output_schema: Option<JsonSchema>,
    pub expected_side_effects: Vec<String>,
    pub required_credentials: Vec<String>,
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
    InputNeeded { description: String },
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
    pub input_data: Vec<serde_json::Value>,
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
