use async_trait::async_trait;
use std::collections::{HashSet, VecDeque};
use tokio::sync::{Mutex, Notify};

use crate::core::namespace::Namespace;
use crate::ports::workflow_queue::{WorkflowQueueItem, WorkflowQueuePort};

pub struct MemoryWorkflowQueue {
    capacity: usize,
    state: Mutex<WorkflowQueueState>,
    not_empty: Notify,
    not_full: Notify,
}

#[derive(Default)]
struct WorkflowQueueState {
    pending: VecDeque<WorkflowQueueItem>,
    pending_ids: HashSet<WorkflowQueueItem>,
    active_ids: HashSet<WorkflowQueueItem>,
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
    async fn enqueue(
        &self,
        namespace: &Namespace,
        workflow_instance_id: String,
    ) -> anyhow::Result<()> {
        let mut item = Some(WorkflowQueueItem {
            namespace: namespace.clone(),
            workflow_instance_id,
        });

        loop {
            let notified = {
                let mut state = self.state.lock().await;
                let item_ref = item.as_ref().unwrap();
                if state.pending_ids.contains(item_ref) {
                    return Ok(());
                }
                if state.pending.len() < self.capacity {
                    let item = item.take().unwrap();
                    state.pending_ids.insert(item.clone());
                    state.pending.push_back(item);
                    self.not_empty.notify_one();
                    return Ok(());
                }

                self.not_full.notified()
            };

            notified.await;
        }
    }

    async fn dequeue(&self) -> anyhow::Result<WorkflowQueueItem> {
        loop {
            let notified = {
                let mut state = self.state.lock().await;
                if let Some(position) = state
                    .pending
                    .iter()
                    .position(|item| !state.active_ids.contains(item))
                {
                    let item = state.pending.remove(position).unwrap();
                    state.pending_ids.remove(&item);
                    state.active_ids.insert(item.clone());
                    self.not_full.notify_one();
                    return Ok(item);
                }

                self.not_empty.notified()
            };

            notified.await;
        }
    }

    async fn complete(
        &self,
        namespace: &Namespace,
        workflow_instance_id: &str,
    ) -> anyhow::Result<()> {
        self.state
            .lock()
            .await
            .active_ids
            .remove(&WorkflowQueueItem {
                namespace: namespace.clone(),
                workflow_instance_id: workflow_instance_id.to_string(),
            });
        self.not_full.notify_one();
        self.not_empty.notify_one();
        Ok(())
    }

    async fn pending_ids(&self, _namespace: &Namespace) -> anyhow::Result<Vec<String>> {
        let state = self.state.lock().await;
        Ok(state
            .pending
            .iter()
            .filter(|item| &item.namespace == _namespace)
            .map(|item| item.workflow_instance_id.clone())
            .collect())
    }

    async fn remove(
        &self,
        namespace: &Namespace,
        workflow_instance_id: &str,
    ) -> anyhow::Result<bool> {
        let mut state = self.state.lock().await;
        let Some(position) = state.pending.iter().position(|item| {
            &item.namespace == namespace
                && item.workflow_instance_id.as_str() == workflow_instance_id
        }) else {
            return Ok(false);
        };

        state.pending.remove(position);
        state.pending_ids.remove(&WorkflowQueueItem {
            namespace: namespace.clone(),
            workflow_instance_id: workflow_instance_id.to_string(),
        });
        self.not_full.notify_one();
        Ok(true)
    }

    async fn purge(&self, namespace: &Namespace) -> anyhow::Result<Vec<String>> {
        let mut state = self.state.lock().await;
        let mut purged = Vec::new();
        state.pending.retain(|item| {
            if &item.namespace == namespace {
                purged.push(item.workflow_instance_id.clone());
                false
            } else {
                true
            }
        });
        state.pending_ids = state.pending.iter().cloned().collect();
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
            .enqueue(
                &crate::core::namespace::test_namespace(),
                "workflow-1".to_string(),
            )
            .await
            .unwrap();
        queue
            .enqueue(
                &crate::core::namespace::test_namespace(),
                "workflow-2".to_string(),
            )
            .await
            .unwrap();

        assert_eq!(
            queue
                .pending_ids(&crate::core::namespace::test_namespace(),)
                .await
                .unwrap(),
            vec!["workflow-1".to_string(), "workflow-2".to_string()]
        );
    }

    #[tokio::test]
    async fn enqueue_ignores_duplicate_pending_id() {
        let queue = MemoryWorkflowQueue::new(10);

        queue
            .enqueue(
                &crate::core::namespace::test_namespace(),
                "workflow-1".to_string(),
            )
            .await
            .unwrap();
        queue
            .enqueue(
                &crate::core::namespace::test_namespace(),
                "workflow-1".to_string(),
            )
            .await
            .unwrap();

        assert_eq!(
            queue
                .pending_ids(&crate::core::namespace::test_namespace(),)
                .await
                .unwrap(),
            vec!["workflow-1".to_string()]
        );
    }

    #[tokio::test]
    async fn active_id_can_be_queued_for_later_pass() {
        let queue = MemoryWorkflowQueue::new(10);

        queue
            .enqueue(
                &crate::core::namespace::test_namespace(),
                "workflow-1".to_string(),
            )
            .await
            .unwrap();
        assert_eq!(
            queue.dequeue().await.unwrap().workflow_instance_id,
            "workflow-1"
        );

        queue
            .enqueue(
                &crate::core::namespace::test_namespace(),
                "workflow-1".to_string(),
            )
            .await
            .unwrap();
        assert_eq!(
            queue
                .pending_ids(&crate::core::namespace::test_namespace(),)
                .await
                .unwrap(),
            vec!["workflow-1".to_string()]
        );

        queue
            .complete(&crate::core::namespace::test_namespace(), "workflow-1")
            .await
            .unwrap();
        assert_eq!(
            queue.dequeue().await.unwrap().workflow_instance_id,
            "workflow-1"
        );
    }

    #[tokio::test]
    async fn dequeue_skips_active_pending_id() {
        let queue = MemoryWorkflowQueue::new(10);

        queue
            .enqueue(
                &crate::core::namespace::test_namespace(),
                "workflow-1".to_string(),
            )
            .await
            .unwrap();
        assert_eq!(
            queue.dequeue().await.unwrap().workflow_instance_id,
            "workflow-1"
        );
        queue
            .enqueue(
                &crate::core::namespace::test_namespace(),
                "workflow-1".to_string(),
            )
            .await
            .unwrap();
        queue
            .enqueue(
                &crate::core::namespace::test_namespace(),
                "workflow-2".to_string(),
            )
            .await
            .unwrap();

        assert_eq!(
            queue.dequeue().await.unwrap().workflow_instance_id,
            "workflow-2"
        );
    }

    #[tokio::test]
    async fn remove_deletes_pending_id() {
        let queue = MemoryWorkflowQueue::new(10);

        queue
            .enqueue(
                &crate::core::namespace::test_namespace(),
                "workflow-1".to_string(),
            )
            .await
            .unwrap();
        queue
            .enqueue(
                &crate::core::namespace::test_namespace(),
                "workflow-2".to_string(),
            )
            .await
            .unwrap();

        assert!(
            queue
                .remove(&crate::core::namespace::test_namespace(), "workflow-1")
                .await
                .unwrap()
        );
        assert!(
            !queue
                .remove(&crate::core::namespace::test_namespace(), "workflow-3")
                .await
                .unwrap()
        );
        assert_eq!(
            queue.dequeue().await.unwrap().workflow_instance_id,
            "workflow-2"
        );
    }

    #[tokio::test]
    async fn purge_deletes_all_pending_ids() {
        let queue = MemoryWorkflowQueue::new(10);

        queue
            .enqueue(
                &crate::core::namespace::test_namespace(),
                "workflow-1".to_string(),
            )
            .await
            .unwrap();
        queue
            .enqueue(
                &crate::core::namespace::test_namespace(),
                "workflow-2".to_string(),
            )
            .await
            .unwrap();

        assert_eq!(
            queue
                .purge(&crate::core::namespace::test_namespace(),)
                .await
                .unwrap(),
            vec!["workflow-1".to_string(), "workflow-2".to_string()]
        );
        assert!(
            queue
                .pending_ids(&crate::core::namespace::test_namespace(),)
                .await
                .unwrap()
                .is_empty()
        );
    }
}
