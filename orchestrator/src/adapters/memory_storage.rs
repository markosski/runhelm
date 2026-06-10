use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::core::models::{FunctionDef, TaskStatus};
use crate::core::workflow::models::{
    VerifierStateStatus, WorkflowDef, WorkflowInstance, WorkflowStatus,
};
use crate::ports::storage::StoragePort;

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
mod tests {}
