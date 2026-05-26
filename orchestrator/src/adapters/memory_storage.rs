use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::core::models::{
    FunctionDef, TaskStatus, VerifierStateStatus, WorkflowDef,
    WorkflowInstance, WorkflowStatus,
};
use crate::ports::storage::{StoragePort, TaskResult, TaskResultMetadata};

pub struct MemoryStorage {
    workflow_defs: RwLock<HashMap<String, WorkflowDef>>,
    function_defs: RwLock<HashMap<String, FunctionDef>>,
    workflow_instances: RwLock<HashMap<String, WorkflowInstance>>,
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

        let metadata = Some(TaskResultMetadata {
            requested_task_id: task_id.to_string(),
            resolved_attempt_id: task_id.to_string(),
            generation: Some(task.generation.clone()),
            verifier_metadata: task.verifier_metadata.clone(),
        });

        match (&task.status, metadata) {
            (TaskStatus::Completed, Some(metadata)) => Ok(TaskResult::SuccessWithMetadata {
                input: task.input_data.clone(),
                output: task.output_data.clone().unwrap_or(serde_json::Value::Null),
                metadata,
            }),
            (TaskStatus::Completed, None) => Ok(TaskResult::Success {
                input: task.input_data.clone(),
                output: task.output_data.clone().unwrap_or(serde_json::Value::Null),
            }),
            (TaskStatus::Failed, Some(metadata)) => Ok(TaskResult::FailureWithMetadata {
                input: task.input_data.clone(),
                error_message: "task failed".to_string(),
                metadata,
            }),
            (TaskStatus::Failed, None) => Ok(TaskResult::Failure {
                input: task.input_data.clone(),
                error_message: "task failed".to_string(),
            }),
            (TaskStatus::Pending, Some(metadata)) => Ok(TaskResult::PendingWithMetadata {
                input: task.input_data.clone(),
                metadata,
            }),
            (TaskStatus::Pending, None) => Ok(TaskResult::Pending {
                input: task.input_data.clone(),
            }),
            (TaskStatus::Running | TaskStatus::InputNeeded { .. }, Some(metadata)) => {
                Ok(TaskResult::RunningWithMetadata {
                    input: task.input_data.clone(),
                    metadata,
                })
            }
            (TaskStatus::Running | TaskStatus::InputNeeded { .. }, None) => {
                Ok(TaskResult::Running {
                    input: task.input_data.clone(),
                })
            }
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
                    || instance
                        .verifier_states
                        .values()
                        .any(|state| state.status == VerifierStateStatus::Running)
            })
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{TaskGenerationMetadata, TaskInstance};
    use serde_json::json;

    fn task_instance(status: TaskStatus, output_data: Option<serde_json::Value>) -> TaskInstance {
        let attempt_id = "task-a[1]".to_string();
        TaskInstance {
            task_def_id: "task-a".to_string(),
            status,
            input_data: vec![json!({"request": true})],
            output_data,
            recorded_side_effects: vec![],
            generation: TaskGenerationMetadata {
                attempt_id,
                original_task_def_id: "task-a".to_string(),
                generation_index: 1,
            },
            verifier_metadata: None,
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
            verifier_states: HashMap::new(),
        };
        storage.save_workflow_instance(instance).await.unwrap();

        match storage
            .get_task_result("instance-1", "completed")
            .await
            .unwrap()
        {
            TaskResult::SuccessWithMetadata {
                input,
                output,
                metadata,
            } => {
                assert_eq!(input, vec![json!({"request": true})]);
                assert_eq!(output, json!({"ok": true}));
                assert_eq!(metadata.generation.unwrap().generation_index, 1);
            }
            result => panic!("expected success with metadata, got {result:?}"),
        }
        assert!(matches!(
            storage
                .get_task_result("instance-1", "pending")
                .await
                .unwrap(),
            TaskResult::PendingWithMetadata { .. }
        ));
        assert!(matches!(
            storage
                .get_task_result("instance-1", "running")
                .await
                .unwrap(),
            TaskResult::RunningWithMetadata { .. }
        ));
        assert!(matches!(
            storage
                .get_task_result("instance-1", "failed")
                .await
                .unwrap(),
            TaskResult::FailureWithMetadata { .. }
        ));
    }

    #[tokio::test]
    async fn list_workflow_instances_returns_completed_and_active_instances() {
        let storage = MemoryStorage::new();
        let completed = WorkflowInstance {
            id: "completed-instance".to_string(),
            workflow_def_id: "workflow-1".to_string(),
            status: WorkflowStatus::Completed,
            tasks: HashMap::new(),
            verifier_states: HashMap::new(),
        };
        let running = WorkflowInstance {
            id: "running-instance".to_string(),
            workflow_def_id: "workflow-1".to_string(),
            status: WorkflowStatus::Running,
            tasks: HashMap::new(),
            verifier_states: HashMap::new(),
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
