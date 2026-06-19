use serde::{Deserialize, Serialize};
use serde_json::Number;

pub type JsonSchema = serde_json::Value;

fn default_true() -> bool {
    true
}

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
        // Whether or not harness session should be re-used across attempts
        #[serde(default = "default_true")]
        reuse_session: bool,
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
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub group_name: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<Workspace>,
    pub required_credentials: Vec<String>,
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

fn default_generation_index() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionMetadata {
    #[serde(default = "default_generation_index")]
    pub generation_index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_context: Option<LoopExecutionContext>,
}

impl Default for ExecutionMetadata {
    fn default() -> Self {
        Self {
            generation_index: default_generation_index(),
            loop_context: None,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn agent_reuse_session_defaults_to_true_when_omitted() {
        let task: TaskDef = serde_json::from_value(json!({
            "id": "agenttask",
            "kind": {
                "Agent": {
                    "model_id": "test/model",
                    "provider_url": "",
                    "prompt": "Do the work.",
                    "tools": [],
                    "skills": [],
                    "ask": false,
                    "schema_failure_retry_times": 0
                }
            },
            "output_schema": null,
            "required_credentials": []
        }))
        .unwrap();

        assert!(matches!(
            task.kind,
            TaskTypeDef::Agent {
                reuse_session: true,
                ..
            }
        ));
    }

    #[test]
    fn agent_reuse_session_serializes_explicit_false() {
        let task: TaskDef = serde_json::from_value(json!({
            "id": "agenttask",
            "kind": {
                "Agent": {
                    "model_id": "test/model",
                    "provider_url": "",
                    "prompt": "Do the work.",
                    "tools": [],
                    "skills": [],
                    "ask": false,
                    "schema_failure_retry_times": 0,
                    "reuse_session": false
                }
            },
            "output_schema": null,
            "required_credentials": []
        }))
        .unwrap();

        let serialized = serde_json::to_value(task).unwrap();
        assert_eq!(serialized["kind"]["Agent"]["reuse_session"], json!(false));
    }

    #[test]
    fn task_workspace_defaults_to_none_when_omitted() {
        let task: TaskDef = serde_json::from_value(json!({
            "id": "task",
            "kind": {
                "Function": {
                    "dependencies": [],
                    "code": "export default async function run() { return {}; }"
                }
            },
            "output_schema": null,
            "required_credentials": []
        }))
        .unwrap();

        assert!(task.workspace.is_none());
    }

    #[test]
    fn task_workspace_deserializes_nested_group_name() {
        let task: TaskDef = serde_json::from_value(json!({
            "id": "task",
            "kind": {
                "Function": {
                    "dependencies": [],
                    "code": "export default async function run() { return {}; }"
                }
            },
            "workspace": {
                "group_name": "repo"
            },
            "output_schema": null,
            "required_credentials": []
        }))
        .unwrap();

        assert_eq!(
            task.workspace
                .as_ref()
                .map(|workspace| workspace.group_name.as_str()),
            Some("repo")
        );
    }

    #[test]
    fn task_workspace_rejects_multiple_group_declaration_fields() {
        let error = serde_json::from_value::<TaskDef>(json!({
            "id": "task",
            "kind": {
                "Function": {
                    "dependencies": [],
                    "code": "export default async function run() { return {}; }"
                }
            },
            "workspace": {
                "group_name": "repo",
                "group": "other"
            },
            "output_schema": null,
            "required_credentials": []
        }))
        .unwrap_err();

        assert!(error.to_string().contains("unknown field `group`"));
    }
}
