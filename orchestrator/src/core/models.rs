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
    #[serde(default)]
    pub timeout_secs: Option<u64>,
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
    pub status: TaskStatus,
    /// True when the task has produced output data.
    pub has_output: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn serialized_public_models_do_not_include_namespace_id() {
        let workflow_def = WorkflowDef {
            id: "workflow-1".to_string(),
            tasks: vec![],
            data_bindings: vec![],
        };
        let function_def = FunctionDef {
            id: "function-1".to_string(),
            dependencies: vec![],
            code: "export default async function run() { return {}; }".to_string(),
        };
        let workflow_instance = WorkflowInstance {
            id: "instance-1".to_string(),
            workflow_def_id: "workflow-1".to_string(),
            status: WorkflowStatus::Pending,
            tasks: HashMap::new(),
        };
        let status_report = WorkflowStatusReport {
            instance_id: "instance-1".to_string(),
            workflow_def_id: "workflow-1".to_string(),
            status: WorkflowStatus::Pending,
            tasks: vec![],
        };
        let workflow_list = WorkflowList {
            workflows: vec![WorkflowSummary {
                id: "instance-1".to_string(),
                workflow_def_id: "workflow-1".to_string(),
                status: WorkflowStatus::Pending,
            }],
        };

        for value in [
            serde_json::to_value(workflow_def).unwrap(),
            serde_json::to_value(function_def).unwrap(),
            serde_json::to_value(workflow_instance).unwrap(),
            serde_json::to_value(status_report).unwrap(),
            serde_json::to_value(workflow_list).unwrap(),
        ] {
            assert_eq!(value.get("namespace_id"), None);
            assert!(!value.to_string().contains("namespace_id"));
        }

        assert_eq!(
            serde_json::to_value(WorkflowQueueStatus {
                pending: vec!["instance-1".to_string()],
            })
            .unwrap(),
            json!({ "pending": ["instance-1"] })
        );
    }
}
