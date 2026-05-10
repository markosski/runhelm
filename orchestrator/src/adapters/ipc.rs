use crate::core::models::TaskDef;
use crate::ports::executor::ExecutionResult;
use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::unix::OwnedWriteHalf;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Mutex, RwLock, oneshot};
use tokio::time;
use tracing::{debug, info, warn};

pub const DEFAULT_SOCKET_PATH: &str = "/tmp/runhelm.sock";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    Idle,
    Busy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerRegistration {
    pub worker_id: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDispatch {
    pub task_id: String,
    pub workflow_def_id: String,
    pub task: TaskDef,
    #[serde(default)]
    pub inputs: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub result: WorkerExecutionResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkerExecutionResult {
    Success { output: serde_json::Value },
    InputNeeded { description: String },
    Failure { reason: String },
}

impl From<ExecutionResult> for WorkerExecutionResult {
    fn from(value: ExecutionResult) -> Self {
        match value {
            ExecutionResult::Success(output) => Self::Success { output },
            ExecutionResult::InputNeeded(description) => Self::InputNeeded { description },
            ExecutionResult::Failure(reason) => Self::Failure { reason },
        }
    }
}

impl From<WorkerExecutionResult> for ExecutionResult {
    fn from(value: WorkerExecutionResult) -> Self {
        match value {
            WorkerExecutionResult::Success { output } => Self::Success(output),
            WorkerExecutionResult::InputNeeded { description } => Self::InputNeeded(description),
            WorkerExecutionResult::Failure { reason } => Self::Failure(reason),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerMessage {
    Register(WorkerRegistration),
    TaskResult(TaskResult),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OrchestratorMessage {
    RegistrationAck { worker_id: String },
    TaskDispatch(TaskDispatch),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WorkerSnapshot {
    pub worker_id: String,
    pub status: WorkerStatus,
}

#[derive(Debug)]
struct WorkerConnection {
    registration: WorkerRegistration,
    status: WorkerStatus,
    current_task_id: Option<String>,
    writer: Arc<Mutex<OwnedWriteHalf>>,
}

#[derive(Debug, Clone, Default)]
pub struct WorkerPool {
    workers: Arc<RwLock<HashMap<String, WorkerConnection>>>,
    result_waiters: Arc<Mutex<HashMap<String, oneshot::Sender<WorkerExecutionResult>>>>,
    next_dispatch_id: Arc<AtomicU64>,
}

impl WorkerPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn add_worker(
        &self,
        registration: WorkerRegistration,
        writer: OwnedWriteHalf,
    ) -> Option<WorkerSnapshot> {
        let snapshot = WorkerSnapshot {
            worker_id: registration.worker_id.clone(),
            status: WorkerStatus::Idle,
        };

        let previous = self
            .workers
            .write()
            .await
            .insert(
                registration.worker_id.clone(),
                WorkerConnection {
                    registration,
                    status: WorkerStatus::Idle,
                    current_task_id: None,
                    writer: Arc::new(Mutex::new(writer)),
                },
            )
            .map(|worker| WorkerSnapshot {
                worker_id: worker.registration.worker_id,
                status: worker.status,
            });

        debug!(worker_id = %snapshot.worker_id, "registered worker");
        previous
    }

    pub async fn remove_worker(&self, worker_id: &str) -> Option<WorkerSnapshot> {
        self.workers
            .write()
            .await
            .remove(worker_id)
            .map(|worker| WorkerSnapshot {
                worker_id: worker.registration.worker_id,
                status: worker.status,
            })
    }

    pub async fn get_worker(&self, worker_id: &str) -> Option<WorkerSnapshot> {
        self.workers
            .read()
            .await
            .get(worker_id)
            .map(|worker| WorkerSnapshot {
                worker_id: worker.registration.worker_id.clone(),
                status: worker.status.clone(),
            })
    }

    #[allow(dead_code)]
    pub async fn len(&self) -> usize {
        self.workers.read().await.len()
    }

    #[allow(dead_code)]
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    pub async fn send_to_worker(
        &self,
        worker_id: &str,
        message: &OrchestratorMessage,
    ) -> anyhow::Result<()> {
        let writer = {
            let workers = self.workers.read().await;
            let worker = workers
                .get(worker_id)
                .with_context(|| format!("worker {worker_id} is not registered"))?;
            Arc::clone(&worker.writer)
        };

        let mut writer = writer.lock().await;
        write_ndjson(&mut *writer, message).await
    }

    pub async fn dispatch_task(
        &self,
        workflow_def_id: &str,
        task: &TaskDef,
        inputs: &[serde_json::Value],
        timeout: Duration,
    ) -> anyhow::Result<ExecutionResult> {
        let task_id = format!(
            "{}-{}",
            task.id,
            self.next_dispatch_id.fetch_add(1, Ordering::Relaxed)
        );
        let (worker_id, writer) = {
            let mut workers = self.workers.write().await;
            let (worker_id, worker) = workers
                .iter_mut()
                .find(|(_, worker)| worker.status == WorkerStatus::Idle)
                .ok_or_else(|| anyhow!("no idle workers available"))?;

            worker.status = WorkerStatus::Busy;
            worker.current_task_id = Some(task_id.clone());
            (worker_id.clone(), Arc::clone(&worker.writer))
        };

        let (result_tx, result_rx) = oneshot::channel();
        self.result_waiters
            .lock()
            .await
            .insert(task_id.clone(), result_tx);

        let dispatch = OrchestratorMessage::TaskDispatch(TaskDispatch {
            task_id: task_id.clone(),
            workflow_def_id: workflow_def_id.to_string(),
            task: task.clone(),
            inputs: inputs.to_vec(),
        });

        let send_result = {
            let mut writer = writer.lock().await;
            write_ndjson(&mut *writer, &dispatch).await
        };

        if let Err(error) = send_result {
            self.result_waiters.lock().await.remove(&task_id);
            self.remove_worker(&worker_id).await;
            return Err(error)
                .with_context(|| format!("dispatch task {task_id} to worker {worker_id}"));
        }

        match time::timeout(timeout, result_rx).await {
            Ok(Ok(result)) => Ok(result.into()),
            Ok(Err(_)) => {
                self.mark_worker_idle_if_current(&worker_id, &task_id).await;
                Err(anyhow!(
                    "worker {worker_id} disconnected before returning task {task_id}"
                ))
            }
            Err(_) => {
                self.result_waiters.lock().await.remove(&task_id);
                self.remove_worker(&worker_id).await;
                Err(anyhow!("task {task_id} timed out after {:?}", timeout))
            }
        }
    }

    async fn complete_task_result(
        &self,
        worker_id: &str,
        result: TaskResult,
    ) -> anyhow::Result<()> {
        let task_id = result.task_id.clone();
        let waiter = self.result_waiters.lock().await.remove(&task_id);
        self.mark_worker_idle_if_current(worker_id, &task_id).await;

        if let Some(waiter) = waiter {
            let _ = waiter.send(result.result);
        } else {
            warn!(worker_id = %worker_id, task_id = %task_id, "ignoring late or untracked worker task result");
        }

        Ok(())
    }

    async fn mark_worker_idle_if_current(&self, worker_id: &str, task_id: &str) {
        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(worker_id) {
            if worker.current_task_id.as_deref() == Some(task_id) {
                worker.status = WorkerStatus::Idle;
                worker.current_task_id = None;
            }
        }
    }

    async fn fail_current_task_for_worker(&self, worker_id: &str) {
        let current_task_id = {
            let workers = self.workers.read().await;
            workers
                .get(worker_id)
                .and_then(|worker| worker.current_task_id.clone())
        };

        if let Some(task_id) = current_task_id {
            if let Some(waiter) = self.result_waiters.lock().await.remove(&task_id) {
                let _ = waiter.send(WorkerExecutionResult::Failure {
                    reason: format!("worker {worker_id} disconnected while running task {task_id}"),
                });
            }
        }
    }
}

pub fn socket_path_from_env() -> PathBuf {
    std::env::var("RUNHELM_SOCKET_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_SOCKET_PATH))
}

pub async fn run_ipc_server(
    socket_path: impl AsRef<Path>,
    worker_pool: WorkerPool,
) -> anyhow::Result<()> {
    let socket_path = socket_path.as_ref();
    if socket_path.exists() {
        tokio::fs::remove_file(socket_path)
            .await
            .with_context(|| format!("remove existing socket {}", socket_path.display()))?;
    }

    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("bind IPC socket {}", socket_path.display()))?;
    tokio::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o660))
        .await
        .with_context(|| format!("set IPC socket permissions {}", socket_path.display()))?;
    info!(socket_path = %socket_path.display(), "IPC server listening");

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .context("accept worker IPC connection")?;
        let worker_pool = worker_pool.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_worker_connection(stream, worker_pool).await {
                warn!(%error, "worker IPC connection closed with error");
            }
        });
    }
}

pub async fn handle_worker_connection(
    stream: UnixStream,
    worker_pool: WorkerPool,
) -> anyhow::Result<()> {
    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    let bytes = reader
        .read_line(&mut line)
        .await
        .context("read worker registration")?;
    if bytes == 0 {
        anyhow::bail!("worker disconnected before registration");
    }

    let registration = match parse_worker_message(&line).context("parse worker registration")? {
        WorkerMessage::Register(registration) => {
            info!(worker_id = %registration.worker_id, "worker connected");
            registration
        }
        WorkerMessage::TaskResult(_) => {
            anyhow::bail!("expected worker registration as first IPC message")
        }
    };

    let worker_id = registration.worker_id.clone();
    worker_pool.add_worker(registration, writer).await;
    worker_pool
        .send_to_worker(
            &worker_id,
            &OrchestratorMessage::RegistrationAck {
                worker_id: worker_id.clone(),
            },
        )
        .await
        .context("send worker registration ack")?;

    loop {
        line.clear();
        let bytes = reader
            .read_line(&mut line)
            .await
            .with_context(|| format!("read message from worker {worker_id}"))?;
        if bytes == 0 {
            break;
        }

        match parse_worker_message(&line) {
            Ok(WorkerMessage::TaskResult(result)) => {
                debug!(worker_id = %worker_id, task_id = %result.task_id, "received worker task result");
                worker_pool
                    .complete_task_result(&worker_id, result)
                    .await
                    .with_context(|| format!("complete task result from worker {worker_id}"))?;
            }
            Ok(WorkerMessage::Register(_)) => {
                warn!(worker_id = %worker_id, "ignoring duplicate worker registration");
            }
            Err(error) => {
                warn!(worker_id = %worker_id, %error, "ignoring malformed worker message");
            }
        }
    }

    worker_pool.fail_current_task_for_worker(&worker_id).await;
    worker_pool.remove_worker(&worker_id).await;
    info!(worker_id = %worker_id, "worker disconnected");
    Ok(())
}

fn parse_worker_message(line: &str) -> anyhow::Result<WorkerMessage> {
    serde_json::from_str(line.trim_end()).context("invalid worker IPC message")
}

async fn write_ndjson<W, T>(writer: &mut W, message: &T) -> anyhow::Result<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let mut payload = serde_json::to_vec(message).context("serialize IPC message")?;
    payload.push(b'\n');
    writer
        .write_all(&payload)
        .await
        .context("write IPC message")?;
    writer.flush().await.context("flush IPC message")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{TaskDef, TaskTypeDef};
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn protocol_serializes_registration_as_ndjson_message() {
        let message = WorkerMessage::Register(WorkerRegistration {
            worker_id: "worker-1".to_string(),
            capabilities: vec!["Function".to_string()],
        });

        let encoded = serde_json::to_string(&message).unwrap();

        assert_eq!(
            encoded,
            r#"{"type":"register","worker_id":"worker-1","capabilities":["Function"]}"#
        );
    }

    #[test]
    fn protocol_serializes_task_dispatch_message() {
        let task = TaskDef {
            id: "task-1".to_string(),
            kind: TaskTypeDef::Function {
                dependencies: vec![],
                code: "return 1".to_string(),
            },
            input_schemas: vec![],
            output_schema: None,
            expected_side_effects: vec![],
            required_credentials: vec![],
        };
        let message = OrchestratorMessage::TaskDispatch(TaskDispatch {
            task_id: "task-1".to_string(),
            workflow_def_id: "workflow-1".to_string(),
            task,
            inputs: vec![json!({"value": 1})],
        });

        let encoded = serde_json::to_value(&message).unwrap();

        assert_eq!(encoded["type"], "task_dispatch");
        assert_eq!(encoded["task_id"], "task-1");
        assert_eq!(encoded["workflow_def_id"], "workflow-1");
        assert_eq!(encoded["inputs"], json!([{"value": 1}]));
    }

    #[tokio::test]
    async fn connection_handler_registers_worker_and_removes_on_disconnect() {
        let socket_path = std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!("runhelm-ipc-test-{}.sock", std::process::id()));
        if socket_path.exists() {
            tokio::fs::remove_file(&socket_path).await.unwrap();
        }

        let listener = UnixListener::bind(&socket_path).unwrap();
        let pool = WorkerPool::new();
        let server_pool = pool.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_worker_connection(stream, server_pool).await.unwrap();
        });

        let mut client = UnixStream::connect(&socket_path).await.unwrap();
        client
            .write_all(br#"{"type":"register","worker_id":"worker-1"}"#)
            .await
            .unwrap();
        client.write_all(b"\n").await.unwrap();

        let mut ack = vec![0; 128];
        let bytes = client.read(&mut ack).await.unwrap();
        let ack = std::str::from_utf8(&ack[..bytes]).unwrap();

        assert!(ack.contains(r#""type":"registration_ack""#));
        assert!(pool.get_worker("worker-1").await.is_some());

        drop(client);
        server.await.unwrap();

        assert!(pool.get_worker("worker-1").await.is_none());
        let _ = tokio::fs::remove_file(&socket_path).await;
    }

    #[tokio::test]
    #[ignore]
    async fn worker_pool_dispatches_tasks_to_multiple_workers() {
        let socket_path = std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!(
                "runhelm-ipc-multi-worker-test-{}.sock",
                std::process::id()
            ));
        if socket_path.exists() {
            tokio::fs::remove_file(&socket_path).await.unwrap();
        }

        let listener = UnixListener::bind(&socket_path).unwrap();
        let pool = WorkerPool::new();
        let server_pool = pool.clone();
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                let server_pool = server_pool.clone();
                tokio::spawn(async move {
                    handle_worker_connection(stream, server_pool).await.unwrap();
                });
            }
        });

        let worker_1 = tokio::spawn(fake_worker(
            socket_path.clone(),
            "worker-1".to_string(),
            json!({"worker": "worker-1"}),
        ));
        let worker_2 = tokio::spawn(fake_worker(
            socket_path.clone(),
            "worker-2".to_string(),
            json!({"worker": "worker-2"}),
        ));

        wait_for_workers(&pool, 2).await;

        let task_1 = test_task("task-1");
        let task_2 = test_task("task-2");
        let (result_1, result_2) = tokio::join!(
            pool.dispatch_task("isolated", &task_1, &[], Duration::from_secs(5)),
            pool.dispatch_task("isolated", &task_2, &[], Duration::from_secs(5)),
        );

        let outputs = [
            unwrap_success(result_1.unwrap()),
            unwrap_success(result_2.unwrap()),
        ];
        assert!(outputs.contains(&json!({"worker": "worker-1"})));
        assert!(outputs.contains(&json!({"worker": "worker-2"})));

        drop(pool);
        worker_1.await.unwrap();
        worker_2.await.unwrap();
        server.await.unwrap();
        let _ = tokio::fs::remove_file(&socket_path).await;
    }

    async fn fake_worker(socket_path: PathBuf, worker_id: String, output: serde_json::Value) {
        let stream = UnixStream::connect(&socket_path).await.unwrap();
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        write_ndjson(
            &mut writer,
            &WorkerMessage::Register(WorkerRegistration {
                worker_id,
                capabilities: vec!["Function".to_string()],
            }),
        )
        .await
        .unwrap();

        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let ack: OrchestratorMessage = serde_json::from_str(line.trim_end()).unwrap();
        assert!(matches!(ack, OrchestratorMessage::RegistrationAck { .. }));

        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let dispatch: OrchestratorMessage = serde_json::from_str(line.trim_end()).unwrap();
        let OrchestratorMessage::TaskDispatch(dispatch) = dispatch else {
            panic!("expected task dispatch");
        };

        write_ndjson(
            &mut writer,
            &WorkerMessage::TaskResult(TaskResult {
                task_id: dispatch.task_id,
                result: WorkerExecutionResult::Success { output },
            }),
        )
        .await
        .unwrap();
    }

    async fn wait_for_workers(pool: &WorkerPool, expected: usize) {
        for _ in 0..50 {
            if pool.len().await == expected {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("expected {expected} workers, found {}", pool.len().await);
    }

    fn test_task(id: &str) -> TaskDef {
        TaskDef {
            id: id.to_string(),
            kind: TaskTypeDef::Function {
                dependencies: vec![],
                code: "return 1".to_string(),
            },
            input_schemas: vec![],
            output_schema: None,
            expected_side_effects: vec![],
            required_credentials: vec![],
        }
    }

    fn unwrap_success(result: ExecutionResult) -> serde_json::Value {
        match result {
            ExecutionResult::Success(output) => output,
            other => panic!("expected success, got {other:?}"),
        }
    }
}
