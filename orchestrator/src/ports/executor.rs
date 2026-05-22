use crate::core::models::{ExecutionMetadata, TaskDef, VerifierExecutionResult};
use async_trait::async_trait;

#[derive(Debug, PartialEq)]
pub enum ExecutionResult {
    Success(serde_json::Value),
    SuccessWithVerifier {
        output: serde_json::Value,
        verifier: VerifierExecutionResult,
    },
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

    async fn execute_with_metadata(
        &self,
        task: &TaskDef,
        inputs: &[serde_json::Value],
        _metadata: &ExecutionMetadata,
    ) -> anyhow::Result<ExecutionResult> {
        self.execute(task, inputs).await
    }
}
