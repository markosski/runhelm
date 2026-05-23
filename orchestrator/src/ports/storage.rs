use crate::core::models::{
    FunctionDef, TaskGenerationMetadata, VerifierAttemptMetadata, WorkflowDef, WorkflowInstance,
};
use async_trait::async_trait;
use serde::Serialize;
use serde::ser::{SerializeMap, Serializer};

#[derive(Debug, Clone, PartialEq)]
pub enum TaskResult {
    Success {
        input: Vec<serde_json::Value>,
        output: serde_json::Value,
    },
    SuccessWithMetadata {
        input: Vec<serde_json::Value>,
        output: serde_json::Value,
        metadata: TaskResultMetadata,
    },
    Failure {
        input: Vec<serde_json::Value>,
        error_message: String,
    },
    FailureWithMetadata {
        input: Vec<serde_json::Value>,
        error_message: String,
        metadata: TaskResultMetadata,
    },
    Pending {
        input: Vec<serde_json::Value>,
    },
    PendingWithMetadata {
        input: Vec<serde_json::Value>,
        metadata: TaskResultMetadata,
    },
    Running {
        input: Vec<serde_json::Value>,
    },
    RunningWithMetadata {
        input: Vec<serde_json::Value>,
        metadata: TaskResultMetadata,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkflowTaskResult {
    pub task_id: String,
    pub result: TaskResult,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskResultMetadata {
    pub requested_task_id: String,
    pub resolved_attempt_id: String,
    pub generation: Option<TaskGenerationMetadata>,
    pub verifier_metadata: Option<VerifierAttemptMetadata>,
}

fn serialize_metadata<S>(
    map: &mut S,
    metadata: &TaskResultMetadata,
) -> Result<(), <S as SerializeMap>::Error>
where
    S: SerializeMap,
{
    map.serialize_entry("requested_task_id", &metadata.requested_task_id)?;
    map.serialize_entry("resolved_attempt_id", &metadata.resolved_attempt_id)?;
    if let Some(generation) = &metadata.generation {
        map.serialize_entry("generation", generation)?;
    }
    if let Some(verifier_metadata) = &metadata.verifier_metadata {
        map.serialize_entry("verifier_metadata", verifier_metadata)?;
    }
    Ok(())
}

impl Serialize for TaskResult {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            TaskResult::Success { input, output } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("status", "success")?;
                map.serialize_entry("input", input)?;
                map.serialize_entry("output", output)?;
                map.end()
            }
            TaskResult::SuccessWithMetadata {
                input,
                output,
                metadata,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("status", "success")?;
                map.serialize_entry("input", input)?;
                map.serialize_entry("output", output)?;
                serialize_metadata(&mut map, metadata)?;
                map.end()
            }
            TaskResult::Failure {
                input,
                error_message,
            } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("status", "failure")?;
                map.serialize_entry("input", input)?;
                map.serialize_entry("error_message", error_message)?;
                map.end()
            }
            TaskResult::FailureWithMetadata {
                input,
                error_message,
                metadata,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("status", "failure")?;
                map.serialize_entry("input", input)?;
                map.serialize_entry("error_message", error_message)?;
                serialize_metadata(&mut map, metadata)?;
                map.end()
            }
            TaskResult::Pending { input } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("status", "pending")?;
                map.serialize_entry("input", input)?;
                map.end()
            }
            TaskResult::PendingWithMetadata { input, metadata } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("status", "pending")?;
                map.serialize_entry("input", input)?;
                serialize_metadata(&mut map, metadata)?;
                map.end()
            }
            TaskResult::Running { input } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("status", "running")?;
                map.serialize_entry("input", input)?;
                map.end()
            }
            TaskResult::RunningWithMetadata { input, metadata } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("status", "running")?;
                map.serialize_entry("input", input)?;
                serialize_metadata(&mut map, metadata)?;
                map.end()
            }
        }
    }
}

#[async_trait]
pub trait StoragePort {
    async fn save_workflow_def(&self, def: WorkflowDef) -> anyhow::Result<()>;
    async fn get_workflow_def(&self, id: &str) -> anyhow::Result<Option<WorkflowDef>>;
    async fn save_function_def(&self, def: FunctionDef) -> anyhow::Result<()>;
    async fn get_function_def(&self, id: &str) -> anyhow::Result<Option<FunctionDef>>;
    async fn delete_function_def(&self, id: &str) -> anyhow::Result<bool>;
    async fn save_workflow_instance(&self, instance: WorkflowInstance) -> anyhow::Result<()>;
    async fn get_workflow_instance(&self, id: &str) -> anyhow::Result<Option<WorkflowInstance>>;
    async fn list_workflow_instances(&self) -> anyhow::Result<Vec<WorkflowInstance>>;
    async fn list_active_workflow_instances(&self) -> anyhow::Result<Vec<WorkflowInstance>>;
    async fn get_task_result(
        &self,
        workflow_instance_id: &str,
        task_id: &str,
    ) -> anyhow::Result<TaskResult>;
}
