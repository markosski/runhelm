use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::core::models::{WorkflowDef, WorkflowInstance};
use crate::ports::storage::StoragePort;

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

    async fn save_workflow_instance(&self, instance: WorkflowInstance) -> anyhow::Result<()> {
        let mut map = self.workflow_instances.write().await;
        map.insert(instance.id.clone(), instance);
        Ok(())
    }

    async fn get_workflow_instance(&self, id: &str) -> anyhow::Result<Option<WorkflowInstance>> {
        let map = self.workflow_instances.read().await;
        Ok(map.get(id).cloned())
    }
}
