use async_trait::async_trait;
use tokio::sync::{Mutex, mpsc};

use crate::ports::workflow_queue::WorkflowQueuePort;

pub struct MemoryWorkflowQueue {
    sender: mpsc::Sender<String>,
    receiver: Mutex<mpsc::Receiver<String>>,
}

impl MemoryWorkflowQueue {
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = mpsc::channel(capacity);
        Self {
            sender,
            receiver: Mutex::new(receiver),
        }
    }
}

#[async_trait]
impl WorkflowQueuePort for MemoryWorkflowQueue {
    async fn enqueue(&self, workflow_instance_id: String) -> anyhow::Result<()> {
        self.sender.send(workflow_instance_id).await?;
        Ok(())
    }

    async fn dequeue(&self) -> anyhow::Result<String> {
        self.receiver
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("workflow queue closed"))
    }
}
