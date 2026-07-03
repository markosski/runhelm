use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::{Mutex, RwLock};

use crate::core::models::FunctionDef;
use crate::core::util::unix_timestamp_ms;
use crate::core::workflow::events::WorkflowEventRecord;
use crate::core::workflow::models::{WorkflowDef, WorkflowInfo, WorkflowInstance, WorkflowStatus};
use crate::ports::storage::{
    StoragePort, StorageResult, WorkflowInfoCursor, WorkflowInfoListRequest, WorkflowInfoPage,
    WorkflowInstanceFilter, WorkflowVersionConflict,
};

pub struct MemoryStorage {
    workflow_defs: RwLock<HashMap<String, WorkflowDef>>,
    function_defs: RwLock<HashMap<String, FunctionDef>>,
    workflow_instances: RwLock<HashMap<String, WorkflowInstance>>,
    workflow_instance_events: RwLock<HashMap<String, Vec<WorkflowEventRecord>>>,
    workflow_infos: RwLock<HashMap<String, WorkflowInfo>>,
    commit_lock: Mutex<()>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            workflow_defs: RwLock::new(HashMap::new()),
            function_defs: RwLock::new(HashMap::new()),
            workflow_instances: RwLock::new(HashMap::new()),
            workflow_instance_events: RwLock::new(HashMap::new()),
            workflow_infos: RwLock::new(HashMap::new()),
            commit_lock: Mutex::new(()),
        }
    }
}

/// This implementation is intended for testing and development purposes only.
/// It is not designed for high-performance or persistent storage.
/// and should not be used in production environments.
#[async_trait]
impl StoragePort for MemoryStorage {
    async fn save_workflow_def(&self, def: WorkflowDef) -> StorageResult<()> {
        let mut map = self.workflow_defs.write().await;
        map.insert(def.id.clone(), def);
        Ok(())
    }

    async fn get_workflow_def(&self, id: &str) -> StorageResult<Option<WorkflowDef>> {
        let map = self.workflow_defs.read().await;
        Ok(map.get(id).cloned())
    }

    async fn save_function_def(&self, def: FunctionDef) -> StorageResult<()> {
        let mut map = self.function_defs.write().await;
        map.insert(def.id.clone(), def);
        Ok(())
    }

    async fn get_function_def(&self, id: &str) -> StorageResult<Option<FunctionDef>> {
        let map = self.function_defs.read().await;
        Ok(map.get(id).cloned())
    }

    async fn delete_function_def(&self, id: &str) -> StorageResult<bool> {
        let mut map = self.function_defs.write().await;
        Ok(map.remove(id).is_some())
    }

    async fn get_workflow_instance(&self, id: &str) -> StorageResult<Option<WorkflowInstance>> {
        let _commit_guard = self.commit_lock.lock().await;
        let map = self.workflow_instances.read().await;
        Ok(map.get(id).cloned())
    }

    async fn get_workflow_instance_events(
        &self,
        workflow_instance_id: &str,
    ) -> StorageResult<Vec<WorkflowEventRecord>> {
        let _commit_guard = self.commit_lock.lock().await;
        let map = self.workflow_instance_events.read().await;
        Ok(map.get(workflow_instance_id).cloned().unwrap_or_default())
    }

    async fn list_workflow_info(
        &self,
        request: WorkflowInfoListRequest,
    ) -> StorageResult<WorkflowInfoPage> {
        let _commit_guard = self.commit_lock.lock().await;
        let map = self.workflow_infos.read().await;
        let mut workflows: Vec<WorkflowInfo> = map
            .values()
            .filter(|info| {
                request
                    .filters
                    .iter()
                    .all(|filter| workflow_info_matches(info, filter))
            })
            .cloned()
            .collect();

        workflows.sort_by(|left, right| {
            right
                .modified_at_epoch_ms
                .cmp(&left.modified_at_epoch_ms)
                .then_with(|| right.id.cmp(&left.id))
        });

        if let Some(cursor) = &request.page.cursor {
            workflows.retain(|info| is_after_cursor(info, cursor));
        }

        let has_more = workflows.len() > request.page.limit;
        workflows.truncate(request.page.limit);
        let next_cursor = has_more
            .then(|| workflows.last())
            .flatten()
            .map(workflow_info_cursor);

        Ok(WorkflowInfoPage {
            workflows,
            next_cursor,
        })
    }

    async fn save_workflow_instance(
        &self,
        expected_version: u64,
        events: Vec<WorkflowEventRecord>,
        instance: WorkflowInstance,
    ) -> StorageResult<()> {
        let _commit_guard = self.commit_lock.lock().await;
        let workflow_instance_id = instance.id.clone();
        let actual_version = self
            .workflow_instances
            .read()
            .await
            .get(&workflow_instance_id)
            .map(|instance| instance.version)
            .unwrap_or(0);

        if actual_version != expected_version {
            return Err(WorkflowVersionConflict {
                workflow_instance_id,
                expected_version,
                actual_version,
            }
            .into());
        }

        let created_from_events_at_epoch_ms = events
            .first()
            .map(|event| event.created_time)
            .unwrap_or(unix_timestamp_ms()?);
        let modified_at_epoch_ms = events
            .last()
            .map(|event| event.created_time)
            .unwrap_or(created_from_events_at_epoch_ms);

        let mut infos = self.workflow_infos.write().await;
        let existing_info = infos.get(&workflow_instance_id);
        let created_at_epoch_ms = existing_info
            .and_then(|info| info.created_at_epoch_ms)
            .or(Some(created_from_events_at_epoch_ms));
        let completed_at_epoch_ms = existing_info
            .and_then(|info| info.completed_at_epoch_ms)
            .or_else(|| workflow_completed_at(&instance, modified_at_epoch_ms));
        let info = WorkflowInfo::from_instance_with_timestamps(
            &instance,
            created_at_epoch_ms,
            modified_at_epoch_ms,
            completed_at_epoch_ms,
        );
        infos.insert(info.id.clone(), info);
        drop(infos);

        let mut events_map = self.workflow_instance_events.write().await;
        events_map
            .entry(workflow_instance_id)
            .or_default()
            .extend(events);
        drop(events_map);

        let mut instances = self.workflow_instances.write().await;
        instances.insert(instance.id.clone(), instance);
        Ok(())
    }
}

fn workflow_info_matches(info: &WorkflowInfo, filter: &WorkflowInstanceFilter) -> bool {
    match filter {
        WorkflowInstanceFilter::Statuses(statuses) => statuses.contains(&info.status),
        WorkflowInstanceFilter::WorkflowDefId(workflow_def_id) => {
            info.workflow_def_id == workflow_def_id.as_str()
        }
    }
}

fn is_after_cursor(info: &WorkflowInfo, cursor: &WorkflowInfoCursor) -> bool {
    info.modified_at_epoch_ms < cursor.modified_at_epoch_ms
        || (info.modified_at_epoch_ms == cursor.modified_at_epoch_ms
            && info.id.as_str() < cursor.workflow_instance_id.as_str())
}

fn workflow_info_cursor(info: &WorkflowInfo) -> WorkflowInfoCursor {
    WorkflowInfoCursor {
        modified_at_epoch_ms: info.modified_at_epoch_ms,
        workflow_instance_id: info.id.clone(),
    }
}

fn workflow_completed_at(instance: &WorkflowInstance, modified_at_epoch_ms: u64) -> Option<u64> {
    matches!(
        instance.status,
        WorkflowStatus::Completed | WorkflowStatus::Failed
    )
    .then_some(modified_at_epoch_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{TaskInstance, TaskSatisfactionStatus, TaskStatus};
    use crate::core::workflow::events::{WorkflowEventRecord, WorkflowInstanceEvent};
    use crate::ports::storage::WorkflowInfoPageRequest;
    use std::collections::HashMap;

    fn instance(id: &str, status: WorkflowStatus) -> WorkflowInstance {
        instance_for_def(id, "wf", status)
    }

    fn instance_for_def(
        id: &str,
        workflow_def_id: &str,
        status: WorkflowStatus,
    ) -> WorkflowInstance {
        WorkflowInstance {
            id: id.to_string(),
            workflow_def_id: workflow_def_id.to_string(),
            version: 0,
            status,
            pinned_worker_host: None,
            tasks: HashMap::new(),
            verifier_states: HashMap::new(),
        }
    }

    fn list_request(filters: Vec<WorkflowInstanceFilter>) -> WorkflowInfoListRequest {
        WorkflowInfoListRequest {
            filters,
            page: WorkflowInfoPageRequest {
                limit: 100,
                cursor: None,
            },
        }
    }

    fn paged_request(
        filters: Vec<WorkflowInstanceFilter>,
        limit: usize,
        cursor: Option<WorkflowInfoCursor>,
    ) -> WorkflowInfoListRequest {
        WorkflowInfoListRequest {
            filters,
            page: WorkflowInfoPageRequest { limit, cursor },
        }
    }

    fn event_record(created_time: u64) -> WorkflowEventRecord {
        WorkflowEventRecord {
            created_time,
            event: WorkflowInstanceEvent::WorkflowStatusChanged {
                status: WorkflowStatus::Running,
            },
        }
    }

    #[tokio::test]
    async fn commits_events_snapshot_and_summary_together() {
        let storage = MemoryStorage::new();
        let mut instance = instance("wf-1", WorkflowStatus::Pending);
        storage
            .save_workflow_instance(0, vec![], instance.clone())
            .await
            .unwrap();

        instance.status = WorkflowStatus::Completed;
        storage
            .save_workflow_instance(
                0,
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
            .list_workflow_info(list_request(vec![WorkflowInstanceFilter::Statuses(vec![
                WorkflowStatus::Completed,
            ])]))
            .await
            .unwrap()
            .workflows;
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].id, "wf-1");
        assert!(infos[0].created_at_epoch_ms.is_some());
        assert!(infos[0].modified_at_epoch_ms >= 42);
        assert_eq!(infos[0].completed_at_epoch_ms, Some(42));
    }

    #[tokio::test]
    async fn rejects_workflow_commit_when_expected_version_is_stale() {
        let storage = MemoryStorage::new();
        let mut instance = instance("wf-1", WorkflowStatus::Pending);
        instance.version = 1;
        storage
            .save_workflow_instance(0, vec![event_record(1000)], instance.clone())
            .await
            .unwrap();

        instance.status = WorkflowStatus::Completed;
        instance.version = 2;
        let error = storage
            .save_workflow_instance(0, vec![event_record(2000)], instance)
            .await
            .unwrap_err();

        let crate::ports::storage::StorageError::WorkflowVersionConflict(conflict) = error else {
            panic!("expected workflow version conflict");
        };
        assert_eq!(conflict.workflow_instance_id, "wf-1");
        assert_eq!(conflict.expected_version, 0);
        assert_eq!(conflict.actual_version, 1);

        let saved = storage
            .get_workflow_instance("wf-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.status, WorkflowStatus::Pending);
        assert_eq!(saved.version, 1);
        assert_eq!(
            storage
                .get_workflow_instance_events("wf-1")
                .await
                .unwrap()
                .len(),
            1
        );
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
                human_input: None,
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
                human_input: None,
                input_data: vec![],
                input_mapping: vec![],
                output_data: None,
                generation_index: 1,
                verifier_metadata: None,
            },
        );

        storage
            .save_workflow_instance(0, vec![], instance.clone())
            .await
            .unwrap();

        let infos = storage
            .list_workflow_info(list_request(vec![]))
            .await
            .unwrap()
            .workflows;
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].total_task_count, 2);
        assert_eq!(infos[0].completed_task_count, 1);
    }

    #[tokio::test]
    async fn summary_timestamps_track_creation_modification_and_completion() {
        let storage = MemoryStorage::new();
        let mut instance = instance("wf-1", WorkflowStatus::Running);
        storage
            .save_workflow_instance(0, vec![event_record(1000)], instance.clone())
            .await
            .unwrap();

        instance.status = WorkflowStatus::Completed;
        storage
            .save_workflow_instance(0, vec![event_record(2000)], instance)
            .await
            .unwrap();

        let infos = storage
            .list_workflow_info(list_request(vec![]))
            .await
            .unwrap()
            .workflows;
        assert_eq!(infos[0].created_at_epoch_ms, Some(1000));
        assert_eq!(infos[0].modified_at_epoch_ms, 2000);
        assert_eq!(infos[0].completed_at_epoch_ms, Some(2000));
    }

    #[tokio::test]
    async fn summary_creation_uses_first_event_and_modification_uses_last_event() {
        let storage = MemoryStorage::new();
        storage
            .save_workflow_instance(
                0,
                vec![event_record(1000), event_record(1500)],
                instance("wf-1", WorkflowStatus::Running),
            )
            .await
            .unwrap();

        let infos = storage
            .list_workflow_info(list_request(vec![]))
            .await
            .unwrap()
            .workflows;
        assert_eq!(infos[0].created_at_epoch_ms, Some(1000));
        assert_eq!(infos[0].modified_at_epoch_ms, 1500);
    }

    #[tokio::test]
    async fn list_workflow_info_sorts_by_modified_time_desc_then_id_desc() {
        let storage = MemoryStorage::new();
        storage
            .save_workflow_instance(
                0,
                vec![event_record(1000)],
                instance("older", WorkflowStatus::Pending),
            )
            .await
            .unwrap();
        storage
            .save_workflow_instance(
                0,
                vec![event_record(2000)],
                instance("same-a", WorkflowStatus::Pending),
            )
            .await
            .unwrap();
        storage
            .save_workflow_instance(
                0,
                vec![event_record(2000)],
                instance("same-b", WorkflowStatus::Pending),
            )
            .await
            .unwrap();

        let infos = storage
            .list_workflow_info(list_request(vec![]))
            .await
            .unwrap()
            .workflows;

        let ids: Vec<&str> = infos.iter().map(|info| info.id.as_str()).collect();
        assert_eq!(ids, vec!["same-b", "same-a", "older"]);
    }

    #[tokio::test]
    async fn list_workflow_info_paginates_after_cursor() {
        let storage = MemoryStorage::new();
        storage
            .save_workflow_instance(
                0,
                vec![event_record(3000)],
                instance("newest", WorkflowStatus::Pending),
            )
            .await
            .unwrap();
        storage
            .save_workflow_instance(
                0,
                vec![event_record(2000)],
                instance("middle", WorkflowStatus::Pending),
            )
            .await
            .unwrap();
        storage
            .save_workflow_instance(
                0,
                vec![event_record(1000)],
                instance("oldest", WorkflowStatus::Pending),
            )
            .await
            .unwrap();

        let first_page = storage
            .list_workflow_info(paged_request(vec![], 1, None))
            .await
            .unwrap();
        assert_eq!(first_page.workflows.len(), 1);
        assert_eq!(first_page.workflows[0].id, "newest");
        assert_eq!(
            first_page.next_cursor,
            Some(WorkflowInfoCursor {
                modified_at_epoch_ms: 3000,
                workflow_instance_id: "newest".to_string(),
            })
        );

        let second_page = storage
            .list_workflow_info(paged_request(vec![], 2, first_page.next_cursor))
            .await
            .unwrap();
        let ids: Vec<&str> = second_page
            .workflows
            .iter()
            .map(|info| info.id.as_str())
            .collect();
        assert_eq!(ids, vec!["middle", "oldest"]);
        assert!(second_page.next_cursor.is_none());
    }

    #[tokio::test]
    async fn filters_summary_queries() {
        let storage = MemoryStorage::new();
        let pending = instance("pending", WorkflowStatus::Pending);
        storage
            .save_workflow_instance(0, vec![], pending.clone())
            .await
            .unwrap();
        let completed = instance("completed", WorkflowStatus::Completed);
        storage
            .save_workflow_instance(0, vec![], completed.clone())
            .await
            .unwrap();

        let active = storage
            .list_workflow_info(list_request(vec![WorkflowInstanceFilter::Statuses(vec![
                WorkflowStatus::Pending,
                WorkflowStatus::Running,
            ])]))
            .await
            .unwrap()
            .workflows;
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "pending");

        let completed = storage
            .list_workflow_info(list_request(vec![WorkflowInstanceFilter::Statuses(vec![
                WorkflowStatus::Completed,
            ])]))
            .await
            .unwrap()
            .workflows;
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].id, "completed");
    }

    #[tokio::test]
    async fn filters_summary_queries_by_workflow_def_id() {
        let storage = MemoryStorage::new();
        storage
            .save_workflow_instance(
                0,
                vec![],
                instance_for_def("workflow-1-instance", "workflow-1", WorkflowStatus::Pending),
            )
            .await
            .unwrap();
        storage
            .save_workflow_instance(
                0,
                vec![],
                instance_for_def(
                    "workflow-2-instance",
                    "workflow-2",
                    WorkflowStatus::Completed,
                ),
            )
            .await
            .unwrap();

        let infos = storage
            .list_workflow_info(list_request(vec![WorkflowInstanceFilter::WorkflowDefId(
                "workflow-2".to_string(),
            )]))
            .await
            .unwrap()
            .workflows;

        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].id, "workflow-2-instance");
    }

    #[tokio::test]
    async fn combines_summary_query_filters_with_and_semantics() {
        let storage = MemoryStorage::new();
        storage
            .save_workflow_instance(
                0,
                vec![],
                instance_for_def("workflow-1-pending", "workflow-1", WorkflowStatus::Pending),
            )
            .await
            .unwrap();
        storage
            .save_workflow_instance(
                0,
                vec![],
                instance_for_def(
                    "workflow-1-completed",
                    "workflow-1",
                    WorkflowStatus::Completed,
                ),
            )
            .await
            .unwrap();
        storage
            .save_workflow_instance(
                0,
                vec![],
                instance_for_def("workflow-2-pending", "workflow-2", WorkflowStatus::Pending),
            )
            .await
            .unwrap();

        let infos = storage
            .list_workflow_info(list_request(vec![
                WorkflowInstanceFilter::WorkflowDefId("workflow-1".to_string()),
                WorkflowInstanceFilter::Statuses(vec![WorkflowStatus::Pending]),
            ]))
            .await
            .unwrap()
            .workflows;

        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].id, "workflow-1-pending");
    }

    #[tokio::test]
    async fn empty_statuses_filter_matches_no_summaries() {
        let storage = MemoryStorage::new();
        storage
            .save_workflow_instance(
                0,
                vec![],
                instance_for_def("workflow-1-pending", "workflow-1", WorkflowStatus::Pending),
            )
            .await
            .unwrap();

        let infos = storage
            .list_workflow_info(list_request(vec![WorkflowInstanceFilter::Statuses(vec![])]))
            .await
            .unwrap()
            .workflows;

        assert!(infos.is_empty());
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
                human_input: None,
                input_data: vec![serde_json::json!({"secret": "input"})],
                input_mapping: vec![],
                output_data: Some(serde_json::json!({"secret": "output"})),
                generation_index: 1,
                verifier_metadata: None,
            },
        );

        storage
            .save_workflow_instance(0, vec![], instance.clone())
            .await
            .unwrap();

        let infos = storage
            .list_workflow_info(list_request(vec![]))
            .await
            .unwrap()
            .workflows;
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].total_task_count, 1);
        assert_eq!(infos[0].completed_task_count, 1);
    }
}
