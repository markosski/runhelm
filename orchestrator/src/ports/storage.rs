use async_trait::async_trait;
use crate::core::models::{WorkflowDef, WorkflowInstance};

#[async_trait]
pub trait StoragePort {
    async fn save_workflow_def(&self, def: WorkflowDef) -> anyhow::Result<()>;
    async fn get_workflow_def(&self, id: &str) -> anyhow::Result<Option<WorkflowDef>>;
    
    async fn save_workflow_instance(&self, instance: WorkflowInstance) -> anyhow::Result<()>;
    async fn get_workflow_instance(&self, id: &str) -> anyhow::Result<Option<WorkflowInstance>>;
}
