use crate::core::models::TaskDef;
use async_trait::async_trait;

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
        task: &TaskDef,
        inputs: &[serde_json::Value],
    ) -> anyhow::Result<ExecutionResult>;
}
