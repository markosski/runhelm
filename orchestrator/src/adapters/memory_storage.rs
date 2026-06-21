use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::core::models::FunctionDef;
use crate::core::workflow::events::WorkflowEventRecord;
use crate::core::workflow::models::{WorkflowDef, WorkflowInfo, WorkflowInstance};
use crate::ports::storage::{StoragePort, WorkflowInstanceFilter};

pub struct MemoryStorage {
    workflow_defs: RwLock<HashMap<String, WorkflowDef>>,
    function_defs: RwLock<HashMap<String, FunctionDef>>,
    workflow_instances: RwLock<HashMap<String, WorkflowInstance>>,
    workflow_instance_events: RwLock<HashMap<String, Vec<WorkflowEventRecord>>>,
    workflow_infos: RwLock<HashMap<String, WorkflowInfo>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            workflow_defs: RwLock::new(HashMap::new()),
            function_defs: RwLock::new(HashMap::new()),
            workflow_instances: RwLock::new(HashMap::new()),
            workflow_instance_events: RwLock::new(HashMap::new()),
            workflow_infos: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl StoragePort for MemoryStorage {
    async fn save_workflow_def(&self, def: WorkflowDef) -> anyhow::Result<()> {
        let mut map = self.workflow_defs.write().await;
        map.insert(def.id.clone(), def);
        Ok(())
    }

    async fn get_workflow_def(&self, id: &str) -> anyhow::Result<Option<WorkflowDef>> {
        let map = self.workflow_defs.read().await;
        Ok(map.get(id).cloned())
    }

    async fn save_function_def(&self, def: FunctionDef) -> anyhow::Result<()> {
        let mut map = self.function_defs.write().await;
        map.insert(def.id.clone(), def);
        Ok(())
    }

    async fn get_function_def(&self, id: &str) -> anyhow::Result<Option<FunctionDef>> {
        let map = self.function_defs.read().await;
        Ok(map.get(id).cloned())
    }

    async fn delete_function_def(&self, id: &str) -> anyhow::Result<bool> {
        let mut map = self.function_defs.write().await;
        Ok(map.remove(id).is_some())
    }

    async fn get_workflow_instance(&self, id: &str) -> anyhow::Result<Option<WorkflowInstance>> {
        let map = self.workflow_instances.read().await;
        Ok(map.get(id).cloned())
    }

    async fn get_workflow_instance_events(
        &self,
        workflow_instance_id: &str,
    ) -> anyhow::Result<Vec<WorkflowEventRecord>> {
        let map = self.workflow_instance_events.read().await;
        Ok(map.get(workflow_instance_id).cloned().unwrap_or_default())
    }

    async fn list_workflow_info(
        &self,
        filter: WorkflowInstanceFilter,
    ) -> anyhow::Result<Vec<WorkflowInfo>> {
        let map = self.workflow_infos.read().await;
        Ok(map
            .values()
            .filter(|info| match &filter {
                WorkflowInstanceFilter::All => true,
                WorkflowInstanceFilter::Status(status) => info.status == *status,
                WorkflowInstanceFilter::Statuses(statuses) => statuses.contains(&info.status),
            })
            .cloned()
            .collect())
    }

    async fn commit_workflow_instance_events(
        &self,
        events: Vec<WorkflowEventRecord>,
        instance: WorkflowInstance,
    ) -> anyhow::Result<()> {
        let info = WorkflowInfo::from_instance(&instance);
        let workflow_instance_id = instance.id.clone();

        let mut events_map = self.workflow_instance_events.write().await;
        events_map
            .entry(workflow_instance_id)
            .or_default()
            .extend(events);
        drop(events_map);

        let mut instances = self.workflow_instances.write().await;
        instances.insert(instance.id.clone(), instance);
        drop(instances);

        let mut infos = self.workflow_infos.write().await;
        infos.insert(info.id.clone(), info);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{TaskInstance, TaskSatisfactionStatus, TaskStatus};
    use crate::core::workflow::events::{WorkflowEventRecord, WorkflowInstanceEvent};
    use crate::core::workflow::models::WorkflowStatus;
    use std::collections::HashMap;

    fn instance(id: &str, status: WorkflowStatus) -> WorkflowInstance {
        WorkflowInstance {
            id: id.to_string(),
            workflow_def_id: "wf".to_string(),
            status,
            tasks: HashMap::new(),
            verifier_states: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn commits_events_snapshot_and_summary_together() {
        let storage = MemoryStorage::new();
        let mut instance = instance("wf-1", WorkflowStatus::Pending);
        storage
            .commit_workflow_instance_events(vec![], instance.clone())
            .await
            .unwrap();

        instance.status = WorkflowStatus::Completed;
        storage
            .commit_workflow_instance_events(
                vec![WorkflowEventRecord {
                    created_time: 42,
                    event: WorkflowInstanceEvent::WorkflowStatusChanged {
                        status: WorkflowStatus::Completed,
                    },
                }],
                instance.clone(),
            )
            .await
            .unwrap();

        let saved = storage
            .get_workflow_instance("wf-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.status, WorkflowStatus::Completed);
        let records = storage.get_workflow_instance_events("wf-1").await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].created_time, 42);
        let infos = storage
            .list_workflow_info(WorkflowInstanceFilter::Status(WorkflowStatus::Completed))
            .await
            .unwrap();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].id, "wf-1");
    }

    #[tokio::test]
    async fn maintains_summary_from_snapshot() {
        let storage = MemoryStorage::new();
        let mut instance = instance("wf-1", WorkflowStatus::Running);
        instance.tasks.insert(
            "task-a[1]".to_string(),
            TaskInstance {
                task_def_id: "task-a".to_string(),
                status: TaskStatus::Completed,
                satisfaction_status: TaskSatisfactionStatus::Pending,
                input_data: vec![],
                input_mapping: vec![],
                output_data: None,
                generation_index: 1,
                verifier_metadata: None,
            },
        );
        instance.tasks.insert(
            "task-b[1]".to_string(),
            TaskInstance {
                task_def_id: "task-b".to_string(),
                status: TaskStatus::Pending,
                satisfaction_status: TaskSatisfactionStatus::Pending,
                input_data: vec![],
                input_mapping: vec![],
                output_data: None,
                generation_index: 1,
                verifier_metadata: None,
            },
        );

        storage
            .commit_workflow_instance_events(vec![], instance.clone())
            .await
            .unwrap();

        let infos = storage
            .list_workflow_info(WorkflowInstanceFilter::All)
            .await
            .unwrap();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].total_task_count, 2);
        assert_eq!(infos[0].completed_task_count, 1);
    }

    #[tokio::test]
    async fn filters_summary_queries() {
        let storage = MemoryStorage::new();
        let pending = instance("pending", WorkflowStatus::Pending);
        storage
            .commit_workflow_instance_events(vec![], pending.clone())
            .await
            .unwrap();
        let completed = instance("completed", WorkflowStatus::Completed);
        storage
            .commit_workflow_instance_events(vec![], completed.clone())
            .await
            .unwrap();

        let active = storage
            .list_workflow_info(WorkflowInstanceFilter::Statuses(vec![
                WorkflowStatus::Pending,
                WorkflowStatus::Running,
            ]))
            .await
            .unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "pending");

        let completed = storage
            .list_workflow_info(WorkflowInstanceFilter::Status(WorkflowStatus::Completed))
            .await
            .unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].id, "completed");
    }

    #[tokio::test]
    async fn summary_listing_does_not_return_full_workflow_state() {
        let storage = MemoryStorage::new();
        let mut instance = instance("wf-1", WorkflowStatus::Completed);
        instance.tasks.insert(
            "task-a[1]".to_string(),
            TaskInstance {
                task_def_id: "task-a".to_string(),
                status: TaskStatus::Completed,
                satisfaction_status: TaskSatisfactionStatus::Pending,
                input_data: vec![serde_json::json!({"secret": "input"})],
                input_mapping: vec![],
                output_data: Some(serde_json::json!({"secret": "output"})),
                generation_index: 1,
                verifier_metadata: None,
            },
        );

        storage
            .commit_workflow_instance_events(vec![], instance.clone())
            .await
            .unwrap();

        let infos = storage
            .list_workflow_info(WorkflowInstanceFilter::All)
            .await
            .unwrap();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].total_task_count, 1);
        assert_eq!(infos[0].completed_task_count, 1);
    }
}
