use crate::core::models::{ExecutionMetadata, TaskDef};
use async_trait::async_trait;
use std::path::Path;

#[derive(Debug, PartialEq)]
pub enum ExecutionResult {
    Success(serde_json::Value),
    InputNeeded(String),
    Failure(String),
}

#[async_trait]
pub trait ExecutorPort {
    async fn execute(
        &self,
        workflow_inst_id: &str,
        task: &TaskDef,
        inputs: &[serde_json::Value],
        metadata: &ExecutionMetadata,
        workspace_path: &Path,
    ) -> anyhow::Result<ExecutionResult>;
}
