use serde::{Deserialize, Serialize};
use serde_json::Number;
use std::collections::HashMap;

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
        // Allowed skills, [] - none, no wildcard support
        skills: Vec<String>,
        // Gives agent allowance to pause task to get additional information if needed
        ask: bool,
        // How many times agent should re-try when output does not match expected output_schema
        schema_failure_retry_times: Number,
    },
    Function(FunctionTaskDef),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDependency {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierControlConfig {
    pub max_iterations: u32,
    pub on_exhausted_continue: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerun_from_task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskControl {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifier: Option<VerifierControlConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FunctionTaskDef {
    Inline {
        // Task will attempt to download these dependencies
        dependencies: Vec<FunctionDependency>,
        code: String,
    },
    Ref {
        #[serde(rename = "ref")]
        reference: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub id: String,
    pub dependencies: Vec<FunctionDependency>,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDef {
    pub id: String,
    pub kind: TaskTypeDef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<TaskControl>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_schemas: Vec<JsonSchema>,
    pub output_schema: Option<JsonSchema>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskGenerationMetadata {
    pub attempt_id: String,
    pub original_task_def_id: String,
    pub generation_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerifierDecision {
    Continue,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerifierAttemptStatus {
    Accepted,
    Rejected,
    ExhaustedAccepted,
    ExhaustedFailed,
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerifierAttemptMetadata {
    pub status: VerifierAttemptStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<VerifierDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verifier_output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInstance {
    pub task_def_id: String,
    pub status: TaskStatus,
    pub input_data: Vec<serde_json::Value>,
    pub output_data: Option<serde_json::Value>,
    pub recorded_side_effects: Vec<SideEffectInstance>,
    pub generation: TaskGenerationMetadata,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifier_metadata: Option<VerifierAttemptMetadata>,
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
pub struct WorkflowInstance {
    pub id: String,
    pub workflow_def_id: String,
    pub status: WorkflowStatus,
    pub tasks: HashMap<String, TaskInstance>,
    #[serde(default)]
    pub verifier_states: HashMap<String, VerifierGenerationState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopExecutionContext {
    pub generation: u32,
    pub max_iterations: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_feedback: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_output: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ExecutionMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_context: Option<LoopExecutionContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerifierExecutionResult {
    pub decision: VerifierDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
    pub output: serde_json::Value,
}

pub fn verifier_decision_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["decision"],
        "properties": {
            "decision": {
                "type": "string",
                "enum": ["complete", "continue"]
            },
            "feedback": {
                "type": "string"
            }
        },
        "additionalProperties": true
    })
}

/// A lightweight read model describing the current state of a workflow instance.
/// Intended for status queries — does not expose raw input/output data.
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
pub struct WorkflowSummary {
    pub id: String,
    pub workflow_def_id: String,
    pub status: WorkflowStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowList {
    pub workflows: Vec<WorkflowSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowQueueStatus {
    pub pending: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusReport {
    pub task_id: String,
    pub task_def_id: String,
    pub status: TaskStatus,
    /// True when the task has produced output data.
    pub has_output: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<TaskGenerationMetadata>,
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
