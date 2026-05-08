use crate::adapters::ipc::WorkerPool;
use crate::core::models::TaskDef;
use crate::ports::executor::{ExecutionResult, ExecutorPort};
use async_trait::async_trait;
use serde_json::Value;
use std::time::Duration;

const DEFAULT_TASK_TIMEOUT: Duration = Duration::from_secs(300);

pub struct DockerExecutor {
    worker_pool: WorkerPool,
    task_timeout: Duration,
}

impl DockerExecutor {
    pub fn new(worker_pool: WorkerPool) -> Self {
        Self {
            worker_pool,
            task_timeout: task_timeout_from_env(),
        }
    }

    #[cfg(test)]
    pub fn with_task_timeout(mut self, task_timeout: Duration) -> Self {
        self.task_timeout = task_timeout;
        self
    }
}

#[async_trait]
impl ExecutorPort for DockerExecutor {
    async fn execute(&self, task: &TaskDef, inputs: &[Value]) -> anyhow::Result<ExecutionResult> {
        self.worker_pool
            .dispatch_task(task, inputs, self.task_timeout)
            .await
    }
}

fn task_timeout_from_env() -> Duration {
    std::env::var("RUNHELM_TASK_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_TASK_TIMEOUT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ipc_backed_executor_fails_when_no_workers_are_available() {
        let executor =
            DockerExecutor::new(WorkerPool::new()).with_task_timeout(Duration::from_millis(10));
        let task = TaskDef {
            id: "task-1".to_string(),
            kind: crate::core::models::TaskTypeDef::Function {
                dependencies: vec![],
                code: "return 1".to_string(),
            },
            input_schemas: vec![],
            output_schema: None,
            expected_side_effects: vec![],
            required_credentials: vec![],
        };

        let error = executor.execute(&task, &[]).await.unwrap_err();

        assert!(error.to_string().contains("no idle workers available"));
    }
}
