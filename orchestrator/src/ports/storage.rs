use crate::core::models::{
    FunctionDef, TaskInputMapping, TaskSatisfactionStatus, VerifierAttemptMetadata,
};
use crate::core::workflow::models::{WorkflowDef, WorkflowInstance};
use async_trait::async_trait;
use serde::Serialize;
use serde::ser::{SerializeMap, Serializer};

#[derive(Debug, Clone, PartialEq)]
pub enum TaskResult {
    Success {
        input: Vec<serde_json::Value>,
        output: serde_json::Value,
        metadata: Option<TaskResultMetadata>,
    },
    Failure {
        input: Vec<serde_json::Value>,
        error_message: String,
        metadata: Option<TaskResultMetadata>,
    },
    Pending {
        input: Vec<serde_json::Value>,
        metadata: Option<TaskResultMetadata>,
    },
    Running {
        input: Vec<serde_json::Value>,
        metadata: Option<TaskResultMetadata>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkflowTaskResult {
    pub task_attempt_id: String,
    pub result: TaskResult,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskResultMetadata {
    pub task_def_id: String,
    pub task_attempt_id: String,
    pub satisfaction: TaskSatisfactionStatus,
    pub input_mapping: Vec<TaskInputMapping>,
    pub generation_index: u32,
    pub verifier_metadata: Option<VerifierAttemptMetadata>,
}

fn serialize_metadata<S>(
    map: &mut S,
    metadata: &TaskResultMetadata,
) -> Result<(), <S as SerializeMap>::Error>
where
    S: SerializeMap,
{
    map.serialize_entry("task_def_id", &metadata.task_def_id)?;
    map.serialize_entry("task_attempt_id", &metadata.task_attempt_id)?;
    map.serialize_entry("satisfaction", &metadata.satisfaction)?;
    if !metadata.input_mapping.is_empty() {
        map.serialize_entry("input_mapping", &metadata.input_mapping)?;
    }
    map.serialize_entry("generation_index", &metadata.generation_index)?;
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
            TaskResult::Success {
                input,
                output,
                metadata,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("status", "success")?;
                map.serialize_entry("input", input)?;
                map.serialize_entry("output", output)?;
                if let Some(metadata) = metadata {
                    serialize_metadata(&mut map, metadata)?;
                }
                map.end()
            }
            TaskResult::Failure {
                input,
                error_message,
                metadata,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("status", "failure")?;
                map.serialize_entry("input", input)?;
                map.serialize_entry("error_message", error_message)?;
                if let Some(metadata) = metadata {
                    serialize_metadata(&mut map, metadata)?;
                }
                map.end()
            }
            TaskResult::Pending { input, metadata } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("status", "pending")?;
                map.serialize_entry("input", input)?;
                if let Some(metadata) = metadata {
                    serialize_metadata(&mut map, metadata)?;
                }
                map.end()
            }
            TaskResult::Running { input, metadata } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("status", "running")?;
                map.serialize_entry("input", input)?;
                if let Some(metadata) = metadata {
                    serialize_metadata(&mut map, metadata)?;
                }
                map.end()
            }
        }
    }
}

#[async_trait]
// TODO: consider converting to using event sourcing for state changes
//  Global destrictive operations like delete workflow should still wipe out all workflow data
pub trait StoragePort {
    async fn get_workflow_def(&self, id: &str) -> anyhow::Result<Option<WorkflowDef>>;
    async fn get_function_def(&self, id: &str) -> anyhow::Result<Option<FunctionDef>>;
    async fn get_workflow_instance(&self, id: &str) -> anyhow::Result<Option<WorkflowInstance>>;
    async fn list_workflow_instances(&self) -> anyhow::Result<Vec<WorkflowInstance>>;
    async fn list_active_workflow_instances(&self) -> anyhow::Result<Vec<WorkflowInstance>>;
    async fn save_workflow_def(&self, def: WorkflowDef) -> anyhow::Result<()>;
    async fn save_function_def(&self, def: FunctionDef) -> anyhow::Result<()>;
    async fn delete_function_def(&self, id: &str) -> anyhow::Result<bool>;
    async fn save_workflow_instance(&self, instance: WorkflowInstance) -> anyhow::Result<()>;
}
