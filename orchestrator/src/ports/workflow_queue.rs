use async_trait::async_trait;

#[async_trait]
pub trait WorkflowQueuePort {
    async fn enqueue(&self, workflow_instance_id: String) -> anyhow::Result<()>;
    async fn dequeue(&self) -> anyhow::Result<String>;
}
