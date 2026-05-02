use async_trait::async_trait;
use crate::core::models::Workflow;

#[async_trait]
pub trait StoragePort {
    async fn save_workflow(&self, workflow: Workflow) -> anyhow::Result<()>;
    async fn get_workflow(&self, id: &str) -> anyhow::Result<Option<Workflow>>;
}
