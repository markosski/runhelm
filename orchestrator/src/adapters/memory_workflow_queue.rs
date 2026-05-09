use async_trait::async_trait;
use std::collections::VecDeque;
use tokio::sync::{Mutex, Notify};

use crate::ports::workflow_queue::WorkflowQueuePort;

pub struct MemoryWorkflowQueue {
    capacity: usize,
    pending: Mutex<VecDeque<String>>,
    not_empty: Notify,
    not_full: Notify,
}

impl MemoryWorkflowQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            pending: Mutex::new(VecDeque::new()),
            not_empty: Notify::new(),
            not_full: Notify::new(),
        }
    }
}

#[async_trait]
impl WorkflowQueuePort for MemoryWorkflowQueue {
    async fn enqueue(&self, workflow_instance_id: String) -> anyhow::Result<()> {
        let mut workflow_instance_id = Some(workflow_instance_id);

        loop {
            let notified = {
                let mut pending = self.pending.lock().await;
                if pending.len() < self.capacity {
                    pending.push_back(workflow_instance_id.take().unwrap());
                    self.not_empty.notify_one();
                    return Ok(());
                }

                self.not_full.notified()
            };

            notified.await;
        }
    }

    async fn dequeue(&self) -> anyhow::Result<String> {
        loop {
            let notified = {
                let mut pending = self.pending.lock().await;
                if let Some(workflow_instance_id) = pending.pop_front() {
                    self.not_full.notify_one();
                    return Ok(workflow_instance_id);
                }

                self.not_empty.notified()
            };

            notified.await;
        }
    }

    async fn pending_ids(&self) -> anyhow::Result<Vec<String>> {
        let pending = self.pending.lock().await;
        Ok(pending.iter().cloned().collect())
    }

    async fn remove(&self, workflow_instance_id: &str) -> anyhow::Result<bool> {
        let mut pending = self.pending.lock().await;
        let Some(position) = pending.iter().position(|id| id == workflow_instance_id) else {
            return Ok(false);
        };

        pending.remove(position);
        self.not_full.notify_one();
        Ok(true)
    }

    async fn purge(&self) -> anyhow::Result<Vec<String>> {
        let mut pending = self.pending.lock().await;
        let purged = pending.drain(..).collect();
        self.not_full.notify_waiters();
        Ok(purged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pending_ids_returns_fifo_snapshot() {
        let queue = MemoryWorkflowQueue::new(10);

        queue.enqueue("workflow-1".to_string()).await.unwrap();
        queue.enqueue("workflow-2".to_string()).await.unwrap();

        assert_eq!(
            queue.pending_ids().await.unwrap(),
            vec!["workflow-1".to_string(), "workflow-2".to_string()]
        );
    }

    #[tokio::test]
    async fn remove_deletes_pending_id() {
        let queue = MemoryWorkflowQueue::new(10);

        queue.enqueue("workflow-1".to_string()).await.unwrap();
        queue.enqueue("workflow-2".to_string()).await.unwrap();

        assert!(queue.remove("workflow-1").await.unwrap());
        assert!(!queue.remove("workflow-3").await.unwrap());
        assert_eq!(queue.dequeue().await.unwrap(), "workflow-2");
    }

    #[tokio::test]
    async fn purge_deletes_all_pending_ids() {
        let queue = MemoryWorkflowQueue::new(10);

        queue.enqueue("workflow-1".to_string()).await.unwrap();
        queue.enqueue("workflow-2".to_string()).await.unwrap();

        assert_eq!(
            queue.purge().await.unwrap(),
            vec!["workflow-1".to_string(), "workflow-2".to_string()]
        );
        assert!(queue.pending_ids().await.unwrap().is_empty());
    }
}
