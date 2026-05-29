use crate::core::models::FunctionDef;
use crate::ports::storage::StoragePort;
use std::sync::Arc;

pub struct FunctionService {
    storage: Arc<dyn StoragePort + Send + Sync>,
}

impl FunctionService {
    pub fn new(storage: Arc<dyn StoragePort + Send + Sync>) -> Self {
        Self { storage }
    }

    pub async fn create_function_def(&self, def: FunctionDef) -> anyhow::Result<()> {
        self.storage.save_function_def(def).await
    }

    pub async fn delete_function_def(&self, id: &str) -> anyhow::Result<bool> {
        self.storage.delete_function_def(id).await
    }
}
