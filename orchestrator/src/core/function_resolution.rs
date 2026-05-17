use crate::core::models::{FunctionTaskDef, TaskDef, TaskTypeDef};
use crate::ports::storage::StoragePort;

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
