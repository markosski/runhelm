use crate::core::models::TaskDef;
use crate::ports::executor::{ExecutionResult, ExecutorPort};
use async_trait::async_trait;
use bollard::Docker;
use bollard::container::LogOutput;
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::{
    AttachContainerOptions, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions,
    WaitContainerOptions,
};
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncWriteExt;

pub struct DockerExecutor {
    client: Docker,
    image: String,
    cmd: Option<Vec<String>>,
}

impl DockerExecutor {
    pub fn new(image: String) -> anyhow::Result<Self> {
        let client = Docker::connect_with_local_defaults()?;
        Ok(Self {
            client,
            image,
            cmd: None,
        })
    }

    #[cfg(test)]
    pub fn with_cmd(mut self, cmd: Vec<String>) -> Self {
        self.cmd = Some(cmd);
        self
    }
}

#[derive(Serialize, Deserialize)]
pub struct TaskInvocationPayload {
    task: TaskDef,
    inputs: Vec<Value>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "status")]
pub enum TaskExecutionResult {
    #[serde(rename = "ok")]
    Ok { output: Value },
    #[serde(rename = "error")]
    Err {
        message: String,
        code: Option<String>,
    },
    #[serde(rename = "input_needed")]
    InputNeeded { description: String },
}

#[async_trait]
impl ExecutorPort for DockerExecutor {
    async fn execute(&self, task: &TaskDef, inputs: &[Value]) -> anyhow::Result<ExecutionResult> {
        let config = ContainerCreateBody {
            image: Some(self.image.clone()),
            cmd: self.cmd.clone(),
            attach_stdin: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            open_stdin: Some(true),
            stdin_once: Some(true),
            ..Default::default()
        };

        // 4.7 Create container
        let create_res = self
            .client
            .create_container(None::<CreateContainerOptions>, config)
            .await?;
        let container_id = create_res.id;

        // Ensure cleanup happens
        let cleanup_client = self.client.clone();
        let cleanup_id = container_id.clone();
        struct CleanupGuard(Docker, String);
        impl Drop for CleanupGuard {
            fn drop(&mut self) {
                let client = self.0.clone();
                let id = self.1.clone();
                tokio::spawn(async move {
                    let _ = client
                        .remove_container(
                            &id,
                            Some(RemoveContainerOptions {
                                force: true,
                                ..Default::default()
                            }),
                        )
                        .await;
                });
            }
        }
        let _guard = CleanupGuard(cleanup_client, cleanup_id);

        // Attach to the container
        let attach_options = AttachContainerOptions {
            stream: true,
            stdin: true,
            stdout: true,
            stderr: true,
            ..Default::default()
        };
        let mut attach_res = self
            .client
            .attach_container(&container_id, Some(attach_options))
            .await?;

        // 4.8 Inject stdin
        let payload = TaskInvocationPayload {
            task: task.clone(),
            inputs: inputs.to_vec(),
        };
        let payload_bytes = serde_json::to_vec(&payload)?;

        let mut stdin = attach_res.input;
        // Write payload
        stdin.write_all(&payload_bytes).await?;
        stdin.flush().await?;
        // Drop stdin to close it (EOF)
        drop(stdin);

        // 4.9 Start container
        self.client
            .start_container(&container_id, None::<StartContainerOptions>)
            .await?;

        // 4.10 Collect stdout/stderr
        let mut stdout_buf = String::new();
        let mut stderr_buf = String::new();

        while let Some(log_res) = attach_res.output.next().await {
            match log_res {
                Ok(LogOutput::StdOut { message }) => {
                    stdout_buf.push_str(&String::from_utf8_lossy(&message));
                }
                Ok(LogOutput::StdErr { message }) => {
                    stderr_buf.push_str(&String::from_utf8_lossy(&message));
                }
                Ok(LogOutput::Console { message }) => {
                    stdout_buf.push_str(&String::from_utf8_lossy(&message));
                }
                Ok(LogOutput::StdIn { .. }) => {}
                Err(e) => anyhow::bail!("Error reading container logs: {}", e),
            }
        }

        // Wait for it to finish (to be safe and get exit code)
        let mut wait_stream = self
            .client
            .wait_container(&container_id, None::<WaitContainerOptions>);
        if let Some(wait_res) = wait_stream.next().await {
            let _wait_info = wait_res?;
        }

        // 4.11 Parse result
        match serde_json::from_str::<TaskExecutionResult>(&stdout_buf) {
            Ok(TaskExecutionResult::Ok { output }) => Ok(ExecutionResult::Success(output)),
            Ok(TaskExecutionResult::InputNeeded { description }) => {
                Ok(ExecutionResult::InputNeeded(description))
            }
            Ok(TaskExecutionResult::Err { message, .. }) => Ok(ExecutionResult::Failure(message)),
            Err(e) => {
                anyhow::bail!(
                    "Failed to parse container output: {}\nStdout: {}\nStderr: {}",
                    e,
                    stdout_buf,
                    stderr_buf
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{TaskDef, TaskTypeDef};
    use serde_json::json;

    #[tokio::test]
    #[ignore]
    async fn test_docker_executor_roundtrip() {
        // We use alpine and override the command to a shell script.
        // The script reads stdin (the JSON payload), ignores it,
        // and echo's back a valid TaskExecutionResult JSON.
        let executor = DockerExecutor::new("alpine:latest".to_string())
            .unwrap()
            .with_cmd(vec![
                "sh".to_string(),
                "-c".to_string(),
                "echo '{\"status\":\"ok\",\"output\":{\"success\":true}}'".to_string(),
            ]);

        let task = TaskDef {
            id: "test-task".to_string(),
            kind: TaskTypeDef::ApiCall {
                url: "http://example.com".to_string(),
                method: "GET".to_string(),
            },
            input_schemas: vec![],
            output_schema: Some(json!({})),
            expected_side_effects: vec![],
            required_credentials: vec![],
        };

        let result = executor.execute(&task, &[]).await.unwrap();
        assert_eq!(result, ExecutionResult::Success(json!({"success": true})));
    }
}
