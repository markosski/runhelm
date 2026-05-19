use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use rusqlite::{Connection, OptionalExtension, params};

use crate::core::models::{FunctionDef, TaskStatus, WorkflowDef, WorkflowInstance, WorkflowStatus};
use crate::ports::storage::{StoragePort, TaskResult};

pub struct SqliteStorage {
    connection: Mutex<Connection>,
}

impl SqliteStorage {
    pub fn init(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let connection = Connection::open(path)?;
        let storage = Self {
            connection: Mutex::new(connection),
        };
        storage.initialize_schema()?;
        Ok(storage)
    }

    fn initialize_schema(&self) -> anyhow::Result<()> {
        let connection = Self::get_conn(&self)?;

        connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS workflow_defs (
                id VARCHAR(255) PRIMARY KEY NOT NULL,
                json TEXT NOT NULL,
                created_at BIGINT NOT NULL,
                updated_at BIGINT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS function_defs (
                id VARCHAR(255) PRIMARY KEY NOT NULL,
                json TEXT NOT NULL,
                created_at BIGINT NOT NULL,
                updated_at BIGINT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS workflow_instance (
                id VARCHAR(255) PRIMARY KEY NOT NULL,
                json TEXT NOT NULL,
                created_at BIGINT NOT NULL,
                updated_at BIGINT NOT NULL
            );
            "#,
        )?;

        Ok(())
    }

    fn get_conn(&self) -> anyhow::Result<MutexGuard<'_, Connection>> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("sqlite connection lock poisoned"))?;

        Ok(connection)
    }

    fn unix_timestamp_secs() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_secs() as i64
    }
}

#[async_trait]
impl StoragePort for SqliteStorage {
    async fn save_workflow_def(&self, def: WorkflowDef) -> anyhow::Result<()> {
        let json = serde_json::to_string(&def)?;
        let now = SqliteStorage::unix_timestamp_secs();

        let connection = self.get_conn()?;

        connection.execute(
            r#"
            INSERT INTO workflow_defs (id, json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(id) DO UPDATE SET
                json = excluded.json,
                updated_at = ?4
            ;"#,
            params![def.id, json, now, now],
        )?;

        Ok(())
    }

    async fn get_workflow_def(&self, id: &str) -> anyhow::Result<Option<WorkflowDef>> {
        let connection = self.get_conn()?;

        let json: Option<String> = connection
            .query_row(
                "SELECT json FROM workflow_defs WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;

        json.map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(Into::into)
    }

    async fn save_function_def(&self, def: FunctionDef) -> anyhow::Result<()> {
        let json = serde_json::to_string(&def)?;
        let now = SqliteStorage::unix_timestamp_secs();

        let connection = self.get_conn()?;

        connection.execute(
            r#"
            INSERT INTO function_defs (id, json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(id) DO UPDATE SET
                json = excluded.json,
                updated_at = ?4
            ;"#,
            params![def.id, json, now, now],
        )?;

        Ok(())
    }

    async fn get_function_def(&self, id: &str) -> anyhow::Result<Option<FunctionDef>> {
        let connection = self.get_conn()?;

        let json: Option<String> = connection
            .query_row(
                "SELECT json FROM function_defs WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;

        json.map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(Into::into)
    }

    async fn delete_function_def(&self, id: &str) -> anyhow::Result<bool> {
        let connection = self.get_conn()?;

        let deleted = connection.execute("DELETE FROM function_defs WHERE id = ?1", params![id])?;

        Ok(deleted > 0)
    }

    async fn save_workflow_instance(&self, instance: WorkflowInstance) -> anyhow::Result<()> {
        let json = serde_json::to_string(&instance)?;
        let now = SqliteStorage::unix_timestamp_secs();
        let connection = self.get_conn()?;

        connection.execute(
            r#"
            INSERT INTO workflow_instance (id, json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(id) DO UPDATE SET
                json = excluded.json,
                updated_at = ?4
            ;"#,
            params![instance.id, json, now, now],
        )?;

        Ok(())
    }

    async fn get_workflow_instance(&self, id: &str) -> anyhow::Result<Option<WorkflowInstance>> {
        let connection = self.get_conn()?;

        let json: Option<String> = connection
            .query_row(
                "SELECT json FROM workflow_instance WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;

        json.map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(Into::into)
    }

    async fn list_workflow_instances(&self) -> anyhow::Result<Vec<WorkflowInstance>> {
        let connection = self.get_conn()?;
        let mut statement = connection.prepare("SELECT json FROM workflow_instance")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;

        let mut instances = Vec::new();
        for row in rows {
            let json = row?;
            instances.push(serde_json::from_str(&json)?);
        }

        Ok(instances)
    }

    async fn list_active_workflow_instances(&self) -> anyhow::Result<Vec<WorkflowInstance>> {
        let instances = self.list_workflow_instances().await?;

        Ok(instances
            .into_iter()
            .filter(|instance| {
                matches!(
                    instance.status,
                    WorkflowStatus::Pending | WorkflowStatus::Running
                ) || instance
                    .tasks
                    .values()
                    .any(|task| matches!(task.status, TaskStatus::Pending | TaskStatus::Running))
            })
            .collect())
    }

    async fn get_task_result(
        &self,
        workflow_instance_id: &str,
        task_id: &str,
    ) -> anyhow::Result<TaskResult> {
        let instance = self
            .get_workflow_instance(workflow_instance_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("workflow instance {workflow_instance_id} not found"))?;
        let task = instance
            .tasks
            .get(task_id)
            .ok_or_else(|| anyhow::anyhow!("task {task_id} not found"))?;

        match &task.status {
            TaskStatus::Completed => Ok(TaskResult::Success(
                task.output_data.clone().unwrap_or(serde_json::Value::Null),
            )),
            TaskStatus::Failed => Ok(TaskResult::Failure {
                error_message: "task failed".to_string(),
            }),
            TaskStatus::Pending => Ok(TaskResult::Pending),
            TaskStatus::Running | TaskStatus::InputNeeded { .. } => Ok(TaskResult::Running),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{
        FunctionDependency, TaskDef, TaskInstance, TaskStatus, TaskTypeDef, WorkflowStatus,
    };
    use serde_json::json;
    use std::collections::HashMap;

    #[tokio::test]
    async fn saves_and_loads_workflow_def() {
        let storage = SqliteStorage::init(":memory:").unwrap();
        let workflow_def = WorkflowDef {
            id: "workflow-1".to_string(),
            tasks: vec![TaskDef {
                id: "task-1".to_string(),
                kind: TaskTypeDef::ApiCall {
                    url: "https://example.com".to_string(),
                    method: "GET".to_string(),
                },
                timeout_secs: Some(30),
                input_schemas: vec![json!({ "type": "object" })],
                output_schema: Some(json!({ "type": "object" })),
                expected_side_effects: vec![],
                required_credentials: vec![],
            }],
            data_bindings: vec![],
        };

        storage
            .save_workflow_def(workflow_def.clone())
            .await
            .unwrap();

        let loaded = storage
            .get_workflow_def("workflow-1")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(loaded.id, workflow_def.id);
        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.tasks[0].id, "task-1");
    }

    #[tokio::test]
    async fn saves_loads_and_deletes_function_def() {
        let storage = SqliteStorage::init(":memory:").unwrap();
        let function_def = FunctionDef {
            id: "function-1".to_string(),
            dependencies: vec![FunctionDependency {
                name: "lodash-es".to_string(),
                version: "4.17.21".to_string(),
            }],
            code: "export default async function run() { return {}; }".to_string(),
        };

        storage
            .save_function_def(function_def.clone())
            .await
            .unwrap();

        let loaded = storage
            .get_function_def("function-1")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(loaded.id, function_def.id);
        assert_eq!(loaded.dependencies.len(), 1);
        assert!(storage.delete_function_def("function-1").await.unwrap());
        assert!(!storage.delete_function_def("function-1").await.unwrap());
        assert!(
            storage
                .get_function_def("function-1")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn saves_loads_lists_and_filters_workflow_instances() {
        let storage = SqliteStorage::init(":memory:").unwrap();
        let completed = workflow_instance("completed-instance", WorkflowStatus::Completed);
        let running = workflow_instance("running-instance", WorkflowStatus::Running);

        storage
            .save_workflow_instance(completed.clone())
            .await
            .unwrap();
        storage
            .save_workflow_instance(running.clone())
            .await
            .unwrap();

        let loaded = storage
            .get_workflow_instance("completed-instance")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.id, completed.id);
        assert_eq!(loaded.status, WorkflowStatus::Completed);

        let mut ids: Vec<String> = storage
            .list_workflow_instances()
            .await
            .unwrap()
            .into_iter()
            .map(|instance| instance.id)
            .collect();
        ids.sort();
        assert_eq!(
            ids,
            vec![
                "completed-instance".to_string(),
                "running-instance".to_string()
            ]
        );

        let active_ids: Vec<String> = storage
            .list_active_workflow_instances()
            .await
            .unwrap()
            .into_iter()
            .map(|instance| instance.id)
            .collect();
        assert_eq!(active_ids, vec!["running-instance".to_string()]);
    }

    #[tokio::test]
    async fn get_task_result_maps_task_state() {
        let storage = SqliteStorage::init(":memory:").unwrap();
        let instance = WorkflowInstance {
            id: "instance-1".to_string(),
            workflow_def_id: "workflow-1".to_string(),
            status: WorkflowStatus::Running,
            tasks: HashMap::from([
                (
                    "completed".to_string(),
                    task_instance(TaskStatus::Completed, Some(json!({"ok": true}))),
                ),
                (
                    "pending".to_string(),
                    task_instance(TaskStatus::Pending, None),
                ),
                (
                    "running".to_string(),
                    task_instance(TaskStatus::Running, None),
                ),
                (
                    "failed".to_string(),
                    task_instance(TaskStatus::Failed, None),
                ),
            ]),
        };
        storage.save_workflow_instance(instance).await.unwrap();

        assert_eq!(
            storage
                .get_task_result("instance-1", "completed")
                .await
                .unwrap(),
            TaskResult::Success(json!({"ok": true}))
        );
        assert_eq!(
            storage
                .get_task_result("instance-1", "pending")
                .await
                .unwrap(),
            TaskResult::Pending
        );
        assert_eq!(
            storage
                .get_task_result("instance-1", "running")
                .await
                .unwrap(),
            TaskResult::Running
        );
        assert_eq!(
            storage
                .get_task_result("instance-1", "failed")
                .await
                .unwrap(),
            TaskResult::Failure {
                error_message: "task failed".to_string()
            }
        );
    }

    fn workflow_instance(id: &str, status: WorkflowStatus) -> WorkflowInstance {
        WorkflowInstance {
            id: id.to_string(),
            workflow_def_id: "workflow-1".to_string(),
            status,
            tasks: HashMap::new(),
        }
    }

    fn task_instance(status: TaskStatus, output_data: Option<serde_json::Value>) -> TaskInstance {
        TaskInstance {
            task_def_id: "task-a".to_string(),
            status,
            input_data: vec![],
            output_data,
            recorded_side_effects: vec![],
        }
    }
}
