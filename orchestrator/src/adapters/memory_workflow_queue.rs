use async_trait::async_trait;
use std::collections::{HashSet, VecDeque};
use tokio::sync::{Mutex, Notify};

use crate::ports::workflow_queue::WorkflowQueuePort;

pub struct MemoryWorkflowQueue {
    capacity: usize,
    state: Mutex<WorkflowQueueState>,
    not_empty: Notify,
    not_full: Notify,
}

#[derive(Default)]
struct WorkflowQueueState {
    pending: VecDeque<String>,
    pending_ids: HashSet<String>,
    active_ids: HashSet<String>,
}

impl MemoryWorkflowQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            state: Mutex::new(WorkflowQueueState::default()),
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
                let mut state = self.state.lock().await;
                let id = workflow_instance_id.as_ref().unwrap();
                if state.pending_ids.contains(id) {
                    return Ok(());
                }
                if state.pending.len() < self.capacity {
                    let id = workflow_instance_id.take().unwrap();
                    state.pending_ids.insert(id.clone());
                    state.pending.push_back(id);
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
                let mut state = self.state.lock().await;
                if let Some(position) = state
                    .pending
                    .iter()
                    .position(|id| !state.active_ids.contains(id))
                {
                    let workflow_instance_id = state.pending.remove(position).unwrap();
                    state.pending_ids.remove(&workflow_instance_id);
                    state.active_ids.insert(workflow_instance_id.clone());
                    self.not_full.notify_one();
                    return Ok(workflow_instance_id);
                }

                self.not_empty.notified()
            };

            notified.await;
        }
    }

    async fn complete(&self, workflow_instance_id: &str) -> anyhow::Result<()> {
        self.state
            .lock()
            .await
            .active_ids
            .remove(workflow_instance_id);
        self.not_full.notify_one();
        self.not_empty.notify_one();
        Ok(())
    }

    async fn pending_ids(&self) -> anyhow::Result<Vec<String>> {
        let state = self.state.lock().await;
        Ok(state.pending.iter().cloned().collect())
    }

    async fn active_ids(&self) -> anyhow::Result<Vec<String>> {
        let state = self.state.lock().await;
        Ok(state.active_ids.iter().cloned().collect())
    }

    async fn remove(&self, workflow_instance_id: &str) -> anyhow::Result<bool> {
        let mut state = self.state.lock().await;
        let Some(position) = state
            .pending
            .iter()
            .position(|id| id == workflow_instance_id)
        else {
            return Ok(false);
        };

        state.pending.remove(position);
        state.pending_ids.remove(workflow_instance_id);
        self.not_full.notify_one();
        Ok(true)
    }

    async fn purge(&self) -> anyhow::Result<Vec<String>> {
        let mut state = self.state.lock().await;
        let purged = state.pending.drain(..).collect();
        state.pending_ids.clear();
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
    async fn enqueue_ignores_duplicate_pending_id() {
        let queue = MemoryWorkflowQueue::new(10);

        queue.enqueue("workflow-1".to_string()).await.unwrap();
        queue.enqueue("workflow-1".to_string()).await.unwrap();

        assert_eq!(
            queue.pending_ids().await.unwrap(),
            vec!["workflow-1".to_string()]
        );
    }

    #[tokio::test]
    async fn active_id_can_be_queued_for_later_pass() {
        let queue = MemoryWorkflowQueue::new(10);

        queue.enqueue("workflow-1".to_string()).await.unwrap();
        assert_eq!(queue.dequeue().await.unwrap(), "workflow-1");

        queue.enqueue("workflow-1".to_string()).await.unwrap();
        assert_eq!(
            queue.pending_ids().await.unwrap(),
            vec!["workflow-1".to_string()]
        );

        queue.complete("workflow-1").await.unwrap();
        assert_eq!(queue.dequeue().await.unwrap(), "workflow-1");
    }

    #[tokio::test]
    async fn dequeue_skips_active_pending_id() {
        let queue = MemoryWorkflowQueue::new(10);

        queue.enqueue("workflow-1".to_string()).await.unwrap();
        assert_eq!(queue.dequeue().await.unwrap(), "workflow-1");
        queue.enqueue("workflow-1".to_string()).await.unwrap();
        queue.enqueue("workflow-2".to_string()).await.unwrap();

        assert_eq!(queue.dequeue().await.unwrap(), "workflow-2");
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
