use crate::core::{
    task::{ExecutionMetadata, TaskDef},
    worker::TaskDispatchConstraints,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    #[serde(
        default,
        rename = "input_provided",
        skip_serializing_if = "Option::is_none"
    )]
    pub human_input_provided: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkerExecutionResult {
    Success { output: serde_json::Value },
    InputNeeded { description: String },
    Failure { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerTaskResult {
    pub task_id: String,
    pub result: WorkerExecutionResult,
}

#[derive(Debug, PartialEq)]
pub enum ExecutionResult {
    Success(serde_json::Value),
    InputNeeded(String),
    Failure(String),
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

#[async_trait]
pub trait TaskDispatchPort {
    async fn dispatch_task(
        &self,
        workflow_inst_id: &str,
        task: &TaskDef,
        inputs: &[serde_json::Value],
        metadata: &ExecutionMetadata,
        constraints: &TaskDispatchConstraints,
    ) -> anyhow::Result<ExecutionResult>;
}
