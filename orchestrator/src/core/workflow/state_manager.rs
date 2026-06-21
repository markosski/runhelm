use crate::core::util::unix_timestamp_ms;
use crate::core::workflow::events::{
    WorkflowEventRecord, WorkflowInstanceEvent, reduce_workflow_instance_events,
};
use crate::core::workflow::models::WorkflowInstance;
use crate::ports::storage::StoragePort;
use std::sync::Arc;

pub struct WorkflowStateManager {
    storage: Arc<dyn StoragePort + Send + Sync>,
}

impl WorkflowStateManager {
    pub fn new(storage: Arc<dyn StoragePort + Send + Sync>) -> Self {
        Self { storage }
    }

    pub async fn commit_events(
        &self,
        workflow_instance_id: &str,
        events: Vec<WorkflowInstanceEvent>,
    ) -> anyhow::Result<WorkflowInstance> {
        let current = self
            .storage
            .get_workflow_instance(workflow_instance_id)
            .await?;
        self.commit_events_for_current(current, events).await
    }

    pub async fn commit_events_for_instance(
        &self,
        current: WorkflowInstance,
        events: Vec<WorkflowInstanceEvent>,
    ) -> anyhow::Result<WorkflowInstance> {
        self.commit_events_for_current(Some(current), events).await
    }

    async fn commit_events_for_current(
        &self,
        current: Option<WorkflowInstance>,
        events: Vec<WorkflowInstanceEvent>,
    ) -> anyhow::Result<WorkflowInstance> {
        if events.is_empty() {
            anyhow::bail!("event batch must not be empty");
        }

        let updated = reduce_workflow_instance_events(current, &events)?;

        let created_time = unix_timestamp_ms()?;
        let records = events
            .into_iter()
            .map(|event| WorkflowEventRecord {
                created_time,
                event,
            })
            .collect();

        self.storage
            .commit_workflow_instance_events(records, updated.clone())
            .await?;

        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::memory_storage::MemoryStorage;
    use crate::core::workflow::events::WorkflowInstanceEvent;
    use crate::core::workflow::models::{WorkflowInstance, WorkflowStatus};
    use crate::ports::storage::{WorkflowInfoListRequest, WorkflowInfoPageRequest};
    use std::collections::HashMap;

    fn list_all_request() -> WorkflowInfoListRequest {
        WorkflowInfoListRequest {
            filters: vec![],
            page: WorkflowInfoPageRequest {
                limit: 100,
                cursor: None,
            },
        }
    }

    #[tokio::test]
    async fn state_manager_rejects_empty_batches() {
        let storage = Arc::new(MemoryStorage::new());
        let manager = WorkflowStateManager::new(storage);

        let result = manager.commit_events("wf-1", vec![]).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn state_manager_persists_events_snapshot_and_summary() {
        let storage = Arc::new(MemoryStorage::new());
        let manager = WorkflowStateManager::new(storage.clone());
        let instance = WorkflowInstance {
            id: "wf-1".to_string(),
            workflow_def_id: "wf".to_string(),
            status: WorkflowStatus::Pending,
            pinned_worker_host: None,
            tasks: HashMap::new(),
            verifier_states: HashMap::new(),
        };

        manager
            .commit_events(
                "wf-1",
                vec![WorkflowInstanceEvent::WorkflowCreated {
                    instance: instance.clone(),
                }],
            )
            .await
            .unwrap();

        let saved = storage
            .get_workflow_instance("wf-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.workflow_def_id, "wf");
        let events = storage.get_workflow_instance_events("wf-1").await.unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].created_time > 0);
        let summaries = storage
            .list_workflow_info(list_all_request())
            .await
            .unwrap()
            .workflows;
        assert_eq!(summaries[0].id, "wf-1");
        assert_eq!(summaries[0].workflow_def_id, "wf");
    }

    #[tokio::test]
    async fn state_manager_commits_events_for_existing_instance_without_loading_first() {
        let storage = Arc::new(MemoryStorage::new());
        let manager = WorkflowStateManager::new(storage.clone());
        let instance = WorkflowInstance {
            id: "wf-1".to_string(),
            workflow_def_id: "wf".to_string(),
            status: WorkflowStatus::Pending,
            pinned_worker_host: None,
            tasks: HashMap::new(),
            verifier_states: HashMap::new(),
        };

        manager
            .commit_events_for_instance(
                instance,
                vec![WorkflowInstanceEvent::WorkflowStatusChanged {
                    status: WorkflowStatus::Running,
                }],
            )
            .await
            .unwrap();

        let saved = storage
            .get_workflow_instance("wf-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.status, WorkflowStatus::Running);
        let events = storage.get_workflow_instance_events("wf-1").await.unwrap();
        assert_eq!(events.len(), 1);
        let summaries = storage
            .list_workflow_info(list_all_request())
            .await
            .unwrap()
            .workflows;
        assert_eq!(summaries[0].status, WorkflowStatus::Running);
    }
}
