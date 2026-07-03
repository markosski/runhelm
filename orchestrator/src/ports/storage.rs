use crate::core::models::{
    FunctionDef, TaskInputMapping, TaskSatisfactionStatus, VerifierAttemptMetadata,
};
use crate::core::workflow::events::WorkflowEventRecord;
use crate::core::workflow::models::{WorkflowDef, WorkflowInfo, WorkflowInstance, WorkflowStatus};
use async_trait::async_trait;
use serde::Serialize;
use serde::ser::{SerializeMap, Serializer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowInstanceFilter {
    Statuses(Vec<WorkflowStatus>),
    WorkflowDefId(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowInfoListRequest {
    pub filters: Vec<WorkflowInstanceFilter>,
    pub page: WorkflowInfoPageRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowInfoPageRequest {
    pub limit: usize,
    pub cursor: Option<WorkflowInfoCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowInfoCursor {
    pub modified_at_epoch_ms: u64,
    pub workflow_instance_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowInfoPage {
    pub workflows: Vec<WorkflowInfo>,
    pub next_cursor: Option<WorkflowInfoCursor>,
}

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
    InputNeeded {
        input: Vec<serde_json::Value>,
        input_request: String,
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
            TaskResult::InputNeeded {
                input,
                input_request,
                metadata,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("status", "input_needed")?;
                map.serialize_entry("input", input)?;
                map.serialize_entry("input_request", input_request)?;
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
    async fn get_workflow_instance_events(
        &self,
        workflow_instance_id: &str,
    ) -> anyhow::Result<Vec<WorkflowEventRecord>>;
    async fn list_workflow_info(
        &self,
        request: WorkflowInfoListRequest,
    ) -> anyhow::Result<WorkflowInfoPage>;
    async fn save_workflow_def(&self, def: WorkflowDef) -> anyhow::Result<()>;
    async fn save_function_def(&self, def: FunctionDef) -> anyhow::Result<()>;
    async fn delete_function_def(&self, id: &str) -> anyhow::Result<bool>;
    async fn commit_workflow_instance_events(
        &self,
        events: Vec<WorkflowEventRecord>,
        instance: WorkflowInstance,
    ) -> anyhow::Result<()>;
}
