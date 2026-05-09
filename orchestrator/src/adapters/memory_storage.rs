use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::core::models::{TaskStatus, WorkflowDef, WorkflowInstance, WorkflowStatus};
use crate::ports::storage::{StoragePort, TaskResult};

pub struct MemoryStorage {
    workflow_defs: RwLock<HashMap<String, WorkflowDef>>,
    workflow_instances: RwLock<HashMap<String, WorkflowInstance>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            workflow_defs: RwLock::new(HashMap::new()),
            workflow_instances: RwLock::new(HashMap::new()),
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

    async fn get_task_result(
        &self,
        workflow_instance_id: &str,
        task_id: &str,
    ) -> anyhow::Result<TaskResult> {
        let map = self.workflow_instances.read().await;
        let instance = map
            .get(workflow_instance_id)
            .ok_or_else(|| anyhow::anyhow!("workflow instance {workflow_instance_id} not found"))?;
        let task = instance
            .tasks
            .get(task_id)
            .ok_or_else(|| anyhow::anyhow!("task {task_id} not found"))?;

        match &task.status {
            TaskStatus::Completed => Ok(TaskResult::Success(
                task.output_data.clone().unwrap_or(serde_json::Value::Null),
            )),
            TaskStatus::Failed => Ok(TaskResult::Failure {
                error_message: "task failed".to_string(),
            }),
            TaskStatus::Pending => Ok(TaskResult::Pending),
            TaskStatus::Running | TaskStatus::InputNeeded { .. } => Ok(TaskResult::Running),
        }
    }

    async fn save_workflow_instance(&self, instance: WorkflowInstance) -> anyhow::Result<()> {
        let mut map = self.workflow_instances.write().await;
        map.insert(instance.id.clone(), instance);
        Ok(())
    }

    async fn get_workflow_instance(&self, id: &str) -> anyhow::Result<Option<WorkflowInstance>> {
        let map = self.workflow_instances.read().await;
        Ok(map.get(id).cloned())
    }

    async fn list_workflow_instances(&self) -> anyhow::Result<Vec<WorkflowInstance>> {
        let map = self.workflow_instances.read().await;
        Ok(map.values().cloned().collect())
    }

    async fn list_active_workflow_instances(&self) -> anyhow::Result<Vec<WorkflowInstance>> {
        let map = self.workflow_instances.read().await;
        Ok(map
            .values()
            .filter(|instance| {
                matches!(
                    instance.status,
                    WorkflowStatus::Pending | WorkflowStatus::Running
                ) || instance
                    .tasks
                    .values()
                    .any(|task| matches!(task.status, TaskStatus::Pending | TaskStatus::Running))
            })
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::TaskInstance;
    use serde_json::json;

    fn task_instance(status: TaskStatus, output_data: Option<serde_json::Value>) -> TaskInstance {
        TaskInstance {
            task_def_id: "task-a".to_string(),
            status,
            input_data: vec![],
            output_data,
            recorded_side_effects: vec![],
        }
    }

    #[tokio::test]
    async fn get_task_result_maps_task_state() {
        let storage = MemoryStorage::new();
        let instance = WorkflowInstance {
            id: "instance-1".to_string(),
            workflow_def_id: "workflow-1".to_string(),
            status: WorkflowStatus::Running,
            tasks: HashMap::from([
                (
                    "completed".to_string(),
                    task_instance(TaskStatus::Completed, Some(json!({"ok": true}))),
                ),
                (
                    "pending".to_string(),
                    task_instance(TaskStatus::Pending, None),
                ),
                (
                    "running".to_string(),
                    task_instance(TaskStatus::Running, None),
                ),
                (
                    "failed".to_string(),
                    task_instance(TaskStatus::Failed, None),
                ),
            ]),
        };
        storage.save_workflow_instance(instance).await.unwrap();

        assert_eq!(
            storage
                .get_task_result("instance-1", "completed")
                .await
                .unwrap(),
            TaskResult::Success(json!({"ok": true}))
        );
        assert_eq!(
            storage
                .get_task_result("instance-1", "pending")
                .await
                .unwrap(),
            TaskResult::Pending
        );
        assert_eq!(
            storage
                .get_task_result("instance-1", "running")
                .await
                .unwrap(),
            TaskResult::Running
        );
        assert_eq!(
            storage
                .get_task_result("instance-1", "failed")
                .await
                .unwrap(),
            TaskResult::Failure {
                error_message: "task failed".to_string()
            }
        );
    }

    #[tokio::test]
    async fn list_workflow_instances_returns_completed_and_active_instances() {
        let storage = MemoryStorage::new();
        let completed = WorkflowInstance {
            id: "completed-instance".to_string(),
            workflow_def_id: "workflow-1".to_string(),
            status: WorkflowStatus::Completed,
            tasks: HashMap::new(),
        };
        let running = WorkflowInstance {
            id: "running-instance".to_string(),
            workflow_def_id: "workflow-1".to_string(),
            status: WorkflowStatus::Running,
            tasks: HashMap::new(),
        };

        storage.save_workflow_instance(completed).await.unwrap();
        storage.save_workflow_instance(running).await.unwrap();

        let mut ids: Vec<String> = storage
            .list_workflow_instances()
            .await
            .unwrap()
            .into_iter()
            .map(|instance| instance.id)
            .collect();
        ids.sort();

        assert_eq!(
            ids,
            vec![
                "completed-instance".to_string(),
                "running-instance".to_string()
            ]
        );
    }
}
