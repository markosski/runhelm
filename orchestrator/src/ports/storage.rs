use crate::core::models::{
    FunctionDef, TaskInputMapping, TaskSatisfactionStatus, VerifierAttemptMetadata,
};
use crate::core::workflow::events::WorkflowEventRecord;
use crate::core::workflow::models::{WorkflowDef, WorkflowInfo, WorkflowInstance, WorkflowStatus};
use async_trait::async_trait;
use serde::Serialize;
use serde::ser::{SerializeMap, Serializer};
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowVersionConflict {
    pub workflow_instance_id: String,
    pub expected_version: u64,
    pub actual_version: u64,
}

impl Display for WorkflowVersionConflict {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "workflow instance {} version conflict: expected {}, actual {}",
            self.workflow_instance_id, self.expected_version, self.actual_version
        )
    }
}

impl Error for WorkflowVersionConflict {}

#[derive(Debug)]
pub enum StorageError {
    WorkflowVersionConflict(WorkflowVersionConflict),
    Backend(anyhow::Error),
}

pub type StorageResult<T> = Result<T, StorageError>;

impl Display for StorageError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WorkflowVersionConflict(conflict) => conflict.fmt(f),
            Self::Backend(error) => write!(f, "storage backend error: {error}"),
        }
    }
}

impl Error for StorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::WorkflowVersionConflict(conflict) => Some(conflict),
            Self::Backend(error) => error.source(),
        }
    }
}

impl From<WorkflowVersionConflict> for StorageError {
    fn from(value: WorkflowVersionConflict) -> Self {
        Self::WorkflowVersionConflict(value)
    }
}

impl From<anyhow::Error> for StorageError {
    fn from(value: anyhow::Error) -> Self {
        Self::Backend(value)
    }
}

impl From<serde_json::Error> for StorageError {
    fn from(value: serde_json::Error) -> Self {
        Self::Backend(value.into())
    }
}

impl From<sqlx::Error> for StorageError {
    fn from(value: sqlx::Error) -> Self {
        Self::Backend(value.into())
    }
}

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
    async fn get_workflow_def(&self, id: &str) -> StorageResult<Option<WorkflowDef>>;
    async fn get_function_def(&self, id: &str) -> StorageResult<Option<FunctionDef>>;
    async fn get_workflow_instance(&self, id: &str) -> StorageResult<Option<WorkflowInstance>>;
    async fn get_workflow_instance_events(
        &self,
        workflow_instance_id: &str,
    ) -> StorageResult<Vec<WorkflowEventRecord>>;
    async fn list_workflow_info(
        &self,
        request: WorkflowInfoListRequest,
    ) -> StorageResult<WorkflowInfoPage>;
    async fn save_workflow_def(&self, def: WorkflowDef) -> StorageResult<()>;
    async fn save_function_def(&self, def: FunctionDef) -> StorageResult<()>;
    async fn delete_function_def(&self, id: &str) -> StorageResult<bool>;
    async fn save_workflow_instance(
        &self,
        expected_version: u64,
        events: Vec<WorkflowEventRecord>,
        instance: WorkflowInstance,
    ) -> StorageResult<()>;
}
