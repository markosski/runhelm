use crate::core::models::FunctionDef;
use crate::ports::storage::StoragePort;
use std::sync::Arc;
use crate::core::models::{FunctionTaskDef, TaskDef, TaskTypeDef};

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

pub async fn resolve_task_function_ref(
    storage: &(dyn StoragePort + Send + Sync),
    task: &TaskDef,
) -> anyhow::Result<TaskDef> {
    let TaskTypeDef::Function(FunctionTaskDef::Ref { reference }) = &task.kind else {
        return Ok(task.clone());
    };

    let Some(function_def) = storage.get_function_def(reference).await? else {
        anyhow::bail!("Function definition not found: {reference}");
    };

    let mut resolved = task.clone();
    resolved.kind = TaskTypeDef::Function(FunctionTaskDef::Inline {
        dependencies: function_def.dependencies,
        code: function_def.code,
    });
    Ok(resolved)
}
