use crate::adapters::worker_pool::WorkerPool;
use crate::core::models::{ExecutionMetadata, TaskDef};
use crate::core::workflow::models::TaskDispatchConstraints;
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
        constraints: &TaskDispatchConstraints,
    ) -> anyhow::Result<ExecutionResult> {
        let task_timeout = task
            .timeout_secs
            .map(Duration::from_secs)
            .unwrap_or(self.task_timeout);

        self.worker_pool
            .enqueue_task(
                workflow_inst_id,
                task,
                inputs,
                task_timeout,
                metadata.clone(),
                constraints.clone(),
            )
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
    use std::path::Path;

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
            workspace: None,
            required_credentials: vec![],
        };

        let result = tokio::time::timeout(
            Duration::from_millis(30),
            executor.execute(
                "123",
                &task,
                &[],
                &ExecutionMetadata::default(),
                &crate::core::workflow::models::TaskDispatchConstraints::default(),
            ),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn task_timeout_overrides_executor_default_timeout() {
        let worker_pool = WorkerPool::new();
        let _timeout_monitor =
            crate::adapters::worker_pool::start_task_timeout_monitor(worker_pool.clone());
        worker_pool
            .register_worker(crate::adapters::worker_pool::WorkerRegistration {
                worker_id: "worker-1".to_string(),
                host_id: crate::core::workflow::models::WorkerHostId::new("test-host"),
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
            workspace: None,
            required_credentials: vec![],
        };

        let execution = tokio::spawn(async move {
            executor
                .execute(
                    "123",
                    &task,
                    &[],
                    &ExecutionMetadata::default(),
                    &crate::core::workflow::models::TaskDispatchConstraints::default(),
                )
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

    #[tokio::test]
    async fn passes_logical_workspace_metadata_to_worker_dispatch() {
        let worker_pool = WorkerPool::new();
        worker_pool
            .register_worker(crate::adapters::worker_pool::WorkerRegistration {
                worker_id: "worker-1".to_string(),
                host_id: crate::core::workflow::models::WorkerHostId::new("test-host"),
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
            timeout_secs: None,
            input_schemas: vec![],
            output_schema: None,
            workspace: None,
            required_credentials: vec![],
        };
        let execution = tokio::spawn(async move {
            executor
                .execute(
                    "workflow-1",
                    &task,
                    &[],
                    &ExecutionMetadata::default(),
                    &crate::core::workflow::models::TaskDispatchConstraints::default(),
                )
                .await
        });

        let claimed = worker_pool
            .claim_task("worker-1", Duration::from_secs(1))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            claimed.workspace_path_suffix,
            Path::new("workflow-1/taskid-task-1")
        );

        worker_pool
            .complete_task_result(
                "worker-1",
                crate::adapters::worker_pool::TaskResult {
                    task_id: claimed.task_id,
                    result: crate::adapters::worker_pool::WorkerExecutionResult::Success {
                        output: serde_json::Value::Null,
                    },
                },
            )
            .await
            .unwrap();

        assert_eq!(
            execution.await.unwrap().unwrap(),
            ExecutionResult::Success(serde_json::Value::Null)
        );
    }

    #[tokio::test]
    async fn passes_dispatch_pin_to_worker_pool_constraints() {
        let worker_pool = WorkerPool::new();
        worker_pool
            .register_worker(crate::adapters::worker_pool::WorkerRegistration {
                worker_id: "worker-a".to_string(),
                host_id: crate::core::workflow::models::WorkerHostId::new("host-a"),
            })
            .await;
        worker_pool
            .register_worker(crate::adapters::worker_pool::WorkerRegistration {
                worker_id: "worker-b".to_string(),
                host_id: crate::core::workflow::models::WorkerHostId::new("host-b"),
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
            timeout_secs: None,
            input_schemas: vec![],
            output_schema: None,
            workspace: None,
            required_credentials: vec![],
        };

        let execution = tokio::spawn(async move {
            executor
                .execute(
                    "workflow-1",
                    &task,
                    &[],
                    &ExecutionMetadata::default(),
                    &TaskDispatchConstraints {
                        pinned_host_id: Some(crate::core::workflow::models::WorkerHostId::new(
                            "host-a",
                        )),
                    },
                )
                .await
        });

        let mismatched_claim = worker_pool
            .claim_task("worker-b", Duration::from_millis(10))
            .await
            .unwrap();
        assert!(mismatched_claim.is_none());

        let claimed = worker_pool
            .claim_task("worker-a", Duration::from_millis(10))
            .await
            .unwrap()
            .unwrap();
        worker_pool
            .complete_task_result(
                "worker-a",
                crate::adapters::worker_pool::TaskResult {
                    task_id: claimed.task_id,
                    result: crate::adapters::worker_pool::WorkerExecutionResult::Success {
                        output: serde_json::Value::Null,
                    },
                },
            )
            .await
            .unwrap();

        assert_eq!(
            execution.await.unwrap().unwrap(),
            ExecutionResult::Success(serde_json::Value::Null)
        );
    }
}
