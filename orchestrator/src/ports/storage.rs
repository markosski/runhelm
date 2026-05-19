use crate::core::models::{FunctionDef, WorkflowDef, WorkflowInstance};
use async_trait::async_trait;
use serde::Serialize;
use serde::ser::{SerializeMap, Serializer};

#[derive(Debug, Clone)]
pub struct NamespacedWorkflowInstance {
    pub namespace_id: String,
    pub instance: WorkflowInstance,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskResult {
    Success(serde_json::Value),
    Failure { error_message: String },
    Pending,
    Running,
}

impl Serialize for TaskResult {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            TaskResult::Success(output) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("status", "success")?;
                map.serialize_entry("output", output)?;
                map.end()
            }
            TaskResult::Failure { error_message } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("status", "failure")?;
                map.serialize_entry("error_message", error_message)?;
                map.end()
            }
            TaskResult::Pending => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("status", "pending")?;
                map.end()
            }
            TaskResult::Running => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("status", "running")?;
                map.end()
            }
        }
    }
}

#[async_trait]
pub trait StoragePort {
    async fn save_workflow_def(&self, namespace_id: &str, def: WorkflowDef) -> anyhow::Result<()>;
    async fn get_workflow_def(
        &self,
        namespace_id: &str,
        id: &str,
    ) -> anyhow::Result<Option<WorkflowDef>>;
    async fn save_function_def(&self, namespace_id: &str, def: FunctionDef) -> anyhow::Result<()>;
    async fn get_function_def(
        &self,
        namespace_id: &str,
        id: &str,
    ) -> anyhow::Result<Option<FunctionDef>>;
    async fn delete_function_def(&self, namespace_id: &str, id: &str) -> anyhow::Result<bool>;
    async fn save_workflow_instance(
        &self,
        namespace_id: &str,
        instance: WorkflowInstance,
    ) -> anyhow::Result<()>;
    async fn get_workflow_instance(
        &self,
        namespace_id: &str,
        id: &str,
    ) -> anyhow::Result<Option<WorkflowInstance>>;
    async fn list_workflow_instances(
        &self,
        namespace_id: &str,
    ) -> anyhow::Result<Vec<WorkflowInstance>>;
    async fn list_active_workflow_instances(
        &self,
    ) -> anyhow::Result<Vec<NamespacedWorkflowInstance>>;
    async fn get_task_result(
        &self,
        namespace_id: &str,
        workflow_instance_id: &str,
        task_id: &str,
    ) -> anyhow::Result<TaskResult>;
}
