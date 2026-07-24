use crate::core::namespace::Namespace;
use async_trait::async_trait;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct WorkflowQueueItem {
    pub namespace: Namespace,
    pub workflow_instance_id: String,
}

#[async_trait]
pub trait WorkflowQueuePort {
    async fn enqueue(
        &self,
        namespace: &Namespace,
        workflow_instance_id: String,
    ) -> anyhow::Result<()>;
    async fn dequeue(&self) -> anyhow::Result<WorkflowQueueItem>;
    async fn complete(
        &self,
        namespace: &Namespace,
        workflow_instance_id: &str,
    ) -> anyhow::Result<()>;
    async fn pending_ids(&self, namespace: &Namespace) -> anyhow::Result<Vec<String>>;
    async fn remove(
        &self,
        namespace: &Namespace,
        workflow_instance_id: &str,
    ) -> anyhow::Result<bool>;
    async fn purge(&self, namespace: &Namespace) -> anyhow::Result<Vec<String>>;
}
