use crate::core::{
    models::{ExecutionMetadata, TaskDef},
    workflow::models::TaskDispatchConstraints,
};
use async_trait::async_trait;

#[derive(Debug, PartialEq)]
pub enum ExecutionResult {
    Success(serde_json::Value),
    InputNeeded(String),
    Failure(String),
}

#[async_trait]
pub trait TaskDispatchPort {
    async fn dispatch_task(
        &self,
        workflow_inst_id: &str,
        task: &TaskDef,
        inputs: &[serde_json::Value],
        metadata: &ExecutionMetadata,
        constraints: &TaskDispatchConstraints,
    ) -> anyhow::Result<ExecutionResult>;
}
