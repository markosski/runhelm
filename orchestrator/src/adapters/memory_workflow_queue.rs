use async_trait::async_trait;
use std::collections::VecDeque;
use tokio::sync::{Mutex, Notify};

use crate::ports::workflow_queue::{QueuedWorkflow, WorkflowQueuePort};

pub struct MemoryWorkflowQueue {
    capacity: usize,
    pending: Mutex<VecDeque<QueuedWorkflow>>,
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
    async fn enqueue(
        &self,
        namespace_id: String,
        workflow_instance_id: String,
    ) -> anyhow::Result<()> {
        let mut item = Some(QueuedWorkflow {
            namespace_id,
            workflow_instance_id,
        });

        loop {
            let notified = {
                let mut pending = self.pending.lock().await;
                if pending.len() < self.capacity {
                    pending.push_back(item.take().unwrap());
                    self.not_empty.notify_one();
                    return Ok(());
                }

                self.not_full.notified()
            };

            notified.await;
        }
    }

    async fn dequeue(&self) -> anyhow::Result<QueuedWorkflow> {
        loop {
            let notified = {
                let mut pending = self.pending.lock().await;
                if let Some(item) = pending.pop_front() {
                    self.not_full.notify_one();
                    return Ok(item);
                }

                self.not_empty.notified()
            };

            notified.await;
        }
    }

    async fn pending_ids(&self, namespace_id: &str) -> anyhow::Result<Vec<String>> {
        let pending = self.pending.lock().await;
        Ok(pending
            .iter()
            .filter(|item| item.namespace_id == namespace_id)
            .map(|item| item.workflow_instance_id.clone())
            .collect())
    }

    async fn remove(&self, namespace_id: &str, workflow_instance_id: &str) -> anyhow::Result<bool> {
        let mut pending = self.pending.lock().await;
        let Some(position) = pending.iter().position(|item| {
            item.namespace_id == namespace_id && item.workflow_instance_id == workflow_instance_id
        }) else {
            return Ok(false);
        };

        pending.remove(position);
        self.not_full.notify_one();
        Ok(true)
    }

    async fn purge(&self, namespace_id: &str) -> anyhow::Result<Vec<String>> {
        let mut pending = self.pending.lock().await;
        let mut purged = Vec::new();
        let mut retained = VecDeque::new();
        while let Some(item) = pending.pop_front() {
            if item.namespace_id == namespace_id {
                purged.push(item.workflow_instance_id);
            } else {
                retained.push_back(item);
            }
        }
        *pending = retained;
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

        queue
            .enqueue("namespace-a".to_string(), "workflow-1".to_string())
            .await
            .unwrap();
        queue
            .enqueue("namespace-b".to_string(), "workflow-2".to_string())
            .await
            .unwrap();

        assert_eq!(
            queue.pending_ids("namespace-a").await.unwrap(),
            vec!["workflow-1".to_string()]
        );
    }

    #[tokio::test]
    async fn remove_deletes_pending_id() {
        let queue = MemoryWorkflowQueue::new(10);

        queue
            .enqueue("namespace-a".to_string(), "workflow-1".to_string())
            .await
            .unwrap();
        queue
            .enqueue("namespace-a".to_string(), "workflow-2".to_string())
            .await
            .unwrap();

        assert!(queue.remove("namespace-a", "workflow-1").await.unwrap());
        assert!(!queue.remove("namespace-a", "workflow-3").await.unwrap());
        assert_eq!(
            queue.dequeue().await.unwrap(),
            QueuedWorkflow {
                namespace_id: "namespace-a".to_string(),
                workflow_instance_id: "workflow-2".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn purge_deletes_all_pending_ids() {
        let queue = MemoryWorkflowQueue::new(10);

        queue
            .enqueue("namespace-a".to_string(), "workflow-1".to_string())
            .await
            .unwrap();
        queue
            .enqueue("namespace-b".to_string(), "workflow-2".to_string())
            .await
            .unwrap();

        assert_eq!(
            queue.purge("namespace-a").await.unwrap(),
            vec!["workflow-1".to_string()]
        );
        assert!(queue.pending_ids("namespace-a").await.unwrap().is_empty());
        assert_eq!(
            queue.pending_ids("namespace-b").await.unwrap(),
            vec!["workflow-2".to_string()]
        );
    }
}
