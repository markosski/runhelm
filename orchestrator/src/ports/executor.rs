use crate::core::models::TaskDef;
use async_trait::async_trait;

#[async_trait]
pub trait ExecutorPort {
    async fn execute(
        &self,
        task: &TaskDef,
        inputs: &[serde_json::Value],
    ) -> anyhow::Result<serde_json::Value>;
}
