use crate::adapters::worker_pool::WorkerPool;
use crate::core::models::{ExecutionMetadata, TaskDef};
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
    async fn execute(
        &self,
        workflow_inst_id: &str,
        task: &TaskDef,
        inputs: &[Value],
        metadata: &ExecutionMetadata,
    ) -> anyhow::Result<ExecutionResult> {
        let task_timeout = task
            .timeout_secs
            .map(Duration::from_secs)
            .unwrap_or(self.task_timeout);

        self.worker_pool
            .enqueue_task(workflow_inst_id, task, inputs, task_timeout, metadata.clone())
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
    async fn worker_pool_backed_executor_waits_when_no_worker_claims_task() {
        let executor =
            DockerExecutor::new(WorkerPool::new()).with_task_timeout(Duration::from_millis(10));
        let task = TaskDef {
            id: "task-1".to_string(),
            kind: crate::core::models::TaskTypeDef::Function(
                crate::core::models::FunctionTaskDef::Inline {
                    dependencies: vec![],
                    code: "return 1".to_string(),
                },
            ),
            control: None,
            timeout_secs: None,
            input_schemas: vec![],
            output_schema: None,
            required_credentials: vec![],
        };

        let result = tokio::time::timeout(
            Duration::from_millis(30),
            executor.execute("123", &task, &[], &ExecutionMetadata::default()),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn task_timeout_overrides_executor_default_timeout() {
        let worker_pool = WorkerPool::new();
        worker_pool
            .register_worker(crate::adapters::worker_pool::WorkerRegistration {
                worker_id: "worker-1".to_string(),
            })
            .await;

        let executor =
            DockerExecutor::new(worker_pool.clone()).with_task_timeout(Duration::from_secs(60));
        let task = TaskDef {
            id: "task-1".to_string(),
            kind: crate::core::models::TaskTypeDef::Function(
                crate::core::models::FunctionTaskDef::Inline {
                    dependencies: vec![],
                    code: "return 1".to_string(),
                },
            ),
            control: None,
            timeout_secs: Some(1),
            input_schemas: vec![],
            output_schema: None,
            required_credentials: vec![],
        };

        let execution = tokio::spawn(async move {
            executor
                .execute("123", &task, &[], &ExecutionMetadata::default())
                .await
        });
        let claimed = worker_pool
            .claim_task("worker-1", Duration::from_secs(1))
            .await
            .unwrap();
        assert!(claimed.is_some());

        let result = execution.await.unwrap().unwrap();

        assert!(matches!(
            result,
            ExecutionResult::Failure(reason) if reason.contains("1s")
        ));
    }
}
