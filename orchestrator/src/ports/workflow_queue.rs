use async_trait::async_trait;

#[async_trait]
pub trait WorkflowQueuePort {
    async fn enqueue(&self, workflow_instance_id: String) -> anyhow::Result<()>;
    async fn dequeue(&self) -> anyhow::Result<String>;
    async fn pending_ids(&self) -> anyhow::Result<Vec<String>>;
    async fn remove(&self, workflow_instance_id: &str) -> anyhow::Result<bool>;
    async fn purge(&self) -> anyhow::Result<Vec<String>>;
}
