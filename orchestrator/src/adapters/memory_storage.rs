use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::core::models::Workflow;
use crate::ports::storage::StoragePort;

pub struct MemoryStorage {
    workflows: RwLock<HashMap<String, Workflow>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            workflows: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl StoragePort for MemoryStorage {
    async fn save_workflow(&self, workflow: Workflow) -> anyhow::Result<()> {
        let mut map = self.workflows.write().await;
        map.insert(workflow.id.clone(), workflow);
        Ok(())
    }

    async fn get_workflow(&self, id: &str) -> anyhow::Result<Option<Workflow>> {
        let map = self.workflows.read().await;
        Ok(map.get(id).cloned())
    }
}
