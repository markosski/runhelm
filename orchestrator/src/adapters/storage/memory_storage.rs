use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::core::models::{FunctionDef, TaskStatus, WorkflowDef, WorkflowInstance, WorkflowStatus};
use crate::ports::storage::{NamespacedWorkflowInstance, StoragePort, TaskResult};

pub struct MemoryStorage {
    workflow_defs: RwLock<HashMap<(String, String), WorkflowDef>>,
    function_defs: RwLock<HashMap<(String, String), FunctionDef>>,
    workflow_instances: RwLock<HashMap<(String, String), WorkflowInstance>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            workflow_defs: RwLock::new(HashMap::new()),
            function_defs: RwLock::new(HashMap::new()),
            workflow_instances: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl StoragePort for MemoryStorage {
    async fn save_workflow_def(&self, namespace_id: &str, def: WorkflowDef) -> anyhow::Result<()> {
        let mut map = self.workflow_defs.write().await;
        map.insert((namespace_id.to_string(), def.id.clone()), def);
        Ok(())
    }

    async fn get_workflow_def(
        &self,
        namespace_id: &str,
        id: &str,
    ) -> anyhow::Result<Option<WorkflowDef>> {
        let map = self.workflow_defs.read().await;
        Ok(map
            .get(&(namespace_id.to_string(), id.to_string()))
            .cloned())
    }

    async fn save_function_def(&self, namespace_id: &str, def: FunctionDef) -> anyhow::Result<()> {
        let mut map = self.function_defs.write().await;
        map.insert((namespace_id.to_string(), def.id.clone()), def);
        Ok(())
    }

    async fn get_function_def(
        &self,
        namespace_id: &str,
        id: &str,
    ) -> anyhow::Result<Option<FunctionDef>> {
        let map = self.function_defs.read().await;
        Ok(map
            .get(&(namespace_id.to_string(), id.to_string()))
            .cloned())
    }

    async fn delete_function_def(&self, namespace_id: &str, id: &str) -> anyhow::Result<bool> {
        let mut map = self.function_defs.write().await;
        Ok(map
            .remove(&(namespace_id.to_string(), id.to_string()))
            .is_some())
    }

    async fn get_task_result(
        &self,
        namespace_id: &str,
        workflow_instance_id: &str,
        task_id: &str,
    ) -> anyhow::Result<TaskResult> {
        let map = self.workflow_instances.read().await;
        let instance = map
            .get(&(namespace_id.to_string(), workflow_instance_id.to_string()))
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

    async fn save_workflow_instance(
        &self,
        namespace_id: &str,
        instance: WorkflowInstance,
    ) -> anyhow::Result<()> {
        let mut map = self.workflow_instances.write().await;
        map.insert((namespace_id.to_string(), instance.id.clone()), instance);
        Ok(())
    }

    async fn get_workflow_instance(
        &self,
        namespace_id: &str,
        id: &str,
    ) -> anyhow::Result<Option<WorkflowInstance>> {
        let map = self.workflow_instances.read().await;
        Ok(map
            .get(&(namespace_id.to_string(), id.to_string()))
            .cloned())
    }

    async fn list_workflow_instances(
        &self,
        namespace_id: &str,
    ) -> anyhow::Result<Vec<WorkflowInstance>> {
        let map = self.workflow_instances.read().await;
        Ok(map
            .iter()
            .filter(|((item_namespace_id, _), _)| item_namespace_id == namespace_id)
            .map(|(_, instance)| instance.clone())
            .collect())
    }

    async fn list_active_workflow_instances(
        &self,
    ) -> anyhow::Result<Vec<NamespacedWorkflowInstance>> {
        let map = self.workflow_instances.read().await;
        Ok(map
            .iter()
            .filter(|(_, instance)| {
                matches!(
                    instance.status,
                    WorkflowStatus::Pending | WorkflowStatus::Running
                ) || instance
                    .tasks
                    .values()
                    .any(|task| matches!(task.status, TaskStatus::Pending | TaskStatus::Running))
            })
            .map(|((namespace_id, _), instance)| NamespacedWorkflowInstance {
                namespace_id: namespace_id.clone(),
                instance: instance.clone(),
            })
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
        storage
            .save_workflow_instance("default", instance)
            .await
            .unwrap();

        assert_eq!(
            storage
                .get_task_result("default", "instance-1", "completed")
                .await
                .unwrap(),
            TaskResult::Success(json!({"ok": true}))
        );
        assert_eq!(
            storage
                .get_task_result("default", "instance-1", "pending")
                .await
                .unwrap(),
            TaskResult::Pending
        );
        assert_eq!(
            storage
                .get_task_result("default", "instance-1", "running")
                .await
                .unwrap(),
            TaskResult::Running
        );
        assert_eq!(
            storage
                .get_task_result("default", "instance-1", "failed")
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

        storage
            .save_workflow_instance("default", completed)
            .await
            .unwrap();
        storage
            .save_workflow_instance("default", running)
            .await
            .unwrap();

        let mut ids: Vec<String> = storage
            .list_workflow_instances("default")
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

    #[tokio::test]
    async fn resources_are_scoped_by_namespace() {
        let storage = MemoryStorage::new();
        let workflow_def = WorkflowDef {
            id: "same-id".to_string(),
            tasks: vec![],
            data_bindings: vec![],
        };
        storage
            .save_workflow_def("namespace-a", workflow_def.clone())
            .await
            .unwrap();
        storage
            .save_workflow_def("namespace-b", workflow_def)
            .await
            .unwrap();

        assert!(
            storage
                .get_workflow_def("namespace-a", "same-id")
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            storage
                .get_workflow_def("namespace-c", "same-id")
                .await
                .unwrap()
                .is_none()
        );

        let function_def = FunctionDef {
            id: "same-id".to_string(),
            dependencies: vec![],
            code: "export default async function run() { return {}; }".to_string(),
        };
        storage
            .save_function_def("namespace-a", function_def.clone())
            .await
            .unwrap();
        storage
            .save_function_def("namespace-b", function_def)
            .await
            .unwrap();
        assert!(
            storage
                .get_function_def("namespace-b", "same-id")
                .await
                .unwrap()
                .is_some()
        );

        let instance = WorkflowInstance {
            id: "same-id".to_string(),
            workflow_def_id: "workflow-1".to_string(),
            status: WorkflowStatus::Pending,
            tasks: HashMap::new(),
        };
        storage
            .save_workflow_instance("namespace-a", instance.clone())
            .await
            .unwrap();
        storage
            .save_workflow_instance("namespace-b", instance)
            .await
            .unwrap();

        assert_eq!(
            storage
                .list_workflow_instances("namespace-a")
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            storage
                .list_workflow_instances("namespace-b")
                .await
                .unwrap()
                .len(),
            1
        );
    }
}
