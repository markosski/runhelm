use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedWorkflow {
    pub namespace_id: String,
    pub workflow_instance_id: String,
}

#[async_trait]
pub trait WorkflowQueuePort {
    async fn enqueue(
        &self,
        namespace_id: String,
        workflow_instance_id: String,
    ) -> anyhow::Result<()>;
    async fn dequeue(&self) -> anyhow::Result<QueuedWorkflow>;
    async fn pending_ids(&self, namespace_id: &str) -> anyhow::Result<Vec<String>>;
    async fn remove(&self, namespace_id: &str, workflow_instance_id: &str) -> anyhow::Result<bool>;
    async fn purge(&self, namespace_id: &str) -> anyhow::Result<Vec<String>>;
}
