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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskSatisfactionStatus {
    #[default]
    Pending,
    Satisfied,
    Unsatisfied,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskInputMapping {
    pub task_id: String,
    pub generation: u32,
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
    #[serde(default)]
    pub satisfaction_status: TaskSatisfactionStatus,
    pub input_data: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_mapping: Vec<TaskInputMapping>,
    pub output_data: Option<serde_json::Value>,
    pub generation_index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifier_metadata: Option<VerifierAttemptMetadata>,
}

impl TaskInstance {
    pub fn make_task_attempt_id(task_def_id: &str, generation_index: u32) -> String {
        format!("{task_def_id}[{generation_index}]")
    }
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
    // Keyed by task_attempt_id, e.g. "task-a[2]".
    pub tasks: HashMap<String, TaskInstance>,
    #[serde(default)]
    pub verifier_states: HashMap<String, VerifierGenerationState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopFeedbackEntry {
    pub generation: u32,
    pub feedback: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopExecutionContext {
    pub generation: u32,
    pub max_iterations: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feedback_history: Vec<LoopFeedbackEntry>,
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
