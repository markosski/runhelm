use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use rusqlite::{Connection, OptionalExtension, params};

use crate::core::models::{FunctionDef, TaskStatus, WorkflowDef, WorkflowInstance, WorkflowStatus};
use crate::ports::auth::DEFAULT_NAMESPACE_ID;
use crate::ports::storage::{NamespacedWorkflowInstance, StoragePort, TaskResult};

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
            CREATE TABLE IF NOT EXISTS namespaces (
                id VARCHAR(255) PRIMARY KEY NOT NULL,
                created_at BIGINT NOT NULL,
                updated_at BIGINT NOT NULL
            );
            "#,
        )?;

        let now = Self::unix_timestamp_secs();
        Self::ensure_namespace(&connection, DEFAULT_NAMESPACE_ID, now)?;

        Self::migrate_table_if_needed(
            &connection,
            "workflow_defs",
            r#"
            CREATE TABLE workflow_defs (
                namespace_id VARCHAR(255) NOT NULL,
                id VARCHAR(255) NOT NULL,
                json TEXT NOT NULL,
                created_at BIGINT NOT NULL,
                updated_at BIGINT NOT NULL,
                PRIMARY KEY (namespace_id, id)
            );
            "#,
            "INSERT INTO workflow_defs (namespace_id, id, json, created_at, updated_at)
             SELECT 'default', id, json, created_at, updated_at FROM workflow_defs_legacy;",
        )?;

        Self::migrate_table_if_needed(
            &connection,
            "function_defs",
            r#"
            CREATE TABLE function_defs (
                namespace_id VARCHAR(255) NOT NULL,
                id VARCHAR(255) NOT NULL,
                json TEXT NOT NULL,
                created_at BIGINT NOT NULL,
                updated_at BIGINT NOT NULL,
                PRIMARY KEY (namespace_id, id)
            );
            "#,
            "INSERT INTO function_defs (namespace_id, id, json, created_at, updated_at)
             SELECT 'default', id, json, created_at, updated_at FROM function_defs_legacy;",
        )?;

        Self::migrate_table_if_needed(
            &connection,
            "workflow_instance",
            r#"
            CREATE TABLE workflow_instance (
                namespace_id VARCHAR(255) NOT NULL,
                id VARCHAR(255) NOT NULL,
                workflow_def_id VARCHAR(255) NOT NULL,
                json TEXT NOT NULL,
                created_at BIGINT NOT NULL,
                updated_at BIGINT NOT NULL,
                PRIMARY KEY (namespace_id, id)
            );
            "#,
            "INSERT INTO workflow_instance (namespace_id, id, workflow_def_id, json, created_at, updated_at)
             SELECT 'default', id, json_extract(json, '$.workflow_def_id'), json, created_at, updated_at
             FROM workflow_instance_legacy;",
        )?;

        connection.execute_batch(
            r#"
            CREATE INDEX IF NOT EXISTS idx_workflow_instance_namespace_updated_at
                ON workflow_instance(namespace_id, updated_at);
            CREATE INDEX IF NOT EXISTS idx_workflow_instance_namespace_workflow_def_id
                ON workflow_instance(namespace_id, workflow_def_id);
            "#,
        )?;

        Ok(())
    }

    fn migrate_table_if_needed(
        connection: &Connection,
        table: &str,
        create_sql: &str,
        copy_sql: &str,
    ) -> anyhow::Result<()> {
        if !Self::table_exists(connection, table)? {
            connection.execute_batch(create_sql)?;
            return Ok(());
        }

        if Self::table_has_column(connection, table, "namespace_id")? {
            return Ok(());
        }

        let legacy_table = format!("{table}_legacy");
        connection.execute_batch(&format!("ALTER TABLE {table} RENAME TO {legacy_table};"))?;
        connection.execute_batch(create_sql)?;
        connection.execute_batch(copy_sql)?;
        connection.execute_batch(&format!("DROP TABLE {legacy_table};"))?;
        Ok(())
    }

    fn table_exists(connection: &Connection, table: &str) -> anyhow::Result<bool> {
        let exists: Option<i64> = connection
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                params![table],
                |row| row.get(0),
            )
            .optional()?;

        Ok(exists.is_some())
    }

    fn table_has_column(
        connection: &Connection,
        table: &str,
        column: &str,
    ) -> anyhow::Result<bool> {
        let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
        let rows = statement.query_map([], |row| row.get::<_, String>(1))?;

        for row in rows {
            if row? == column {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn ensure_namespace(
        connection: &Connection,
        namespace_id: &str,
        now: i64,
    ) -> anyhow::Result<()> {
        connection.execute(
            r#"
            INSERT INTO namespaces (id, created_at, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(id) DO UPDATE SET updated_at = excluded.updated_at;
            "#,
            params![namespace_id, now, now],
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
    async fn save_workflow_def(&self, namespace_id: &str, def: WorkflowDef) -> anyhow::Result<()> {
        let json = serde_json::to_string(&def)?;
        let now = SqliteStorage::unix_timestamp_secs();

        let connection = self.get_conn()?;
        Self::ensure_namespace(&connection, namespace_id, now)?;

        connection.execute(
            r#"
            INSERT INTO workflow_defs (namespace_id, id, json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(namespace_id, id) DO UPDATE SET
                json = excluded.json,
                updated_at = excluded.updated_at
            ;"#,
            params![namespace_id, def.id, json, now, now],
        )?;

        Ok(())
    }

    async fn get_workflow_def(
        &self,
        namespace_id: &str,
        id: &str,
    ) -> anyhow::Result<Option<WorkflowDef>> {
        let connection = self.get_conn()?;

        let json: Option<String> = connection
            .query_row(
                "SELECT json FROM workflow_defs WHERE namespace_id = ?1 AND id = ?2",
                params![namespace_id, id],
                |row| row.get(0),
            )
            .optional()?;

        json.map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(Into::into)
    }

    async fn save_function_def(&self, namespace_id: &str, def: FunctionDef) -> anyhow::Result<()> {
        let json = serde_json::to_string(&def)?;
        let now = SqliteStorage::unix_timestamp_secs();

        let connection = self.get_conn()?;
        Self::ensure_namespace(&connection, namespace_id, now)?;

        connection.execute(
            r#"
            INSERT INTO function_defs (namespace_id, id, json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(namespace_id, id) DO UPDATE SET
                json = excluded.json,
                updated_at = excluded.updated_at
            ;"#,
            params![namespace_id, def.id, json, now, now],
        )?;

        Ok(())
    }

    async fn get_function_def(
        &self,
        namespace_id: &str,
        id: &str,
    ) -> anyhow::Result<Option<FunctionDef>> {
        let connection = self.get_conn()?;

        let json: Option<String> = connection
            .query_row(
                "SELECT json FROM function_defs WHERE namespace_id = ?1 AND id = ?2",
                params![namespace_id, id],
                |row| row.get(0),
            )
            .optional()?;

        json.map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(Into::into)
    }

    async fn delete_function_def(&self, namespace_id: &str, id: &str) -> anyhow::Result<bool> {
        let connection = self.get_conn()?;

        let deleted = connection.execute(
            "DELETE FROM function_defs WHERE namespace_id = ?1 AND id = ?2",
            params![namespace_id, id],
        )?;

        Ok(deleted > 0)
    }

    async fn save_workflow_instance(
        &self,
        namespace_id: &str,
        instance: WorkflowInstance,
    ) -> anyhow::Result<()> {
        let json = serde_json::to_string(&instance)?;
        let now = SqliteStorage::unix_timestamp_secs();
        let connection = self.get_conn()?;
        Self::ensure_namespace(&connection, namespace_id, now)?;

        connection.execute(
            r#"
            INSERT INTO workflow_instance (namespace_id, id, workflow_def_id, json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(namespace_id, id) DO UPDATE SET
                workflow_def_id = excluded.workflow_def_id,
                json = excluded.json,
                updated_at = excluded.updated_at
            ;"#,
            params![
                namespace_id,
                instance.id,
                instance.workflow_def_id,
                json,
                now,
                now
            ],
        )?;

        Ok(())
    }

    async fn get_workflow_instance(
        &self,
        namespace_id: &str,
        id: &str,
    ) -> anyhow::Result<Option<WorkflowInstance>> {
        let connection = self.get_conn()?;

        let json: Option<String> = connection
            .query_row(
                "SELECT json FROM workflow_instance WHERE namespace_id = ?1 AND id = ?2",
                params![namespace_id, id],
                |row| row.get(0),
            )
            .optional()?;

        json.map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(Into::into)
    }

    async fn list_workflow_instances(
        &self,
        namespace_id: &str,
    ) -> anyhow::Result<Vec<WorkflowInstance>> {
        let connection = self.get_conn()?;
        let mut statement =
            connection.prepare("SELECT json FROM workflow_instance WHERE namespace_id = ?1")?;
        let rows = statement.query_map(params![namespace_id], |row| row.get::<_, String>(0))?;

        let mut instances = Vec::new();
        for row in rows {
            let json = row?;
            instances.push(serde_json::from_str(&json)?);
        }

        Ok(instances)
    }

    async fn list_active_workflow_instances(
        &self,
    ) -> anyhow::Result<Vec<NamespacedWorkflowInstance>> {
        let connection = self.get_conn()?;
        let mut statement =
            connection.prepare("SELECT namespace_id, json FROM workflow_instance")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut instances = Vec::new();
        for row in rows {
            let (namespace_id, json) = row?;
            instances.push(NamespacedWorkflowInstance {
                namespace_id,
                instance: serde_json::from_str(&json)?,
            });
        }

        Ok(instances
            .into_iter()
            .filter(|item| {
                matches!(
                    item.instance.status,
                    WorkflowStatus::Pending | WorkflowStatus::Running
                ) || item
                    .instance
                    .tasks
                    .values()
                    .any(|task| matches!(task.status, TaskStatus::Pending | TaskStatus::Running))
            })
            .collect())
    }

    async fn get_task_result(
        &self,
        namespace_id: &str,
        workflow_instance_id: &str,
        task_id: &str,
    ) -> anyhow::Result<TaskResult> {
        let instance = self
            .get_workflow_instance(namespace_id, workflow_instance_id)
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
            .save_workflow_def("default", workflow_def.clone())
            .await
            .unwrap();

        let loaded = storage
            .get_workflow_def("default", "workflow-1")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(loaded.id, workflow_def.id);
        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.tasks[0].id, "task-1");
    }

    #[tokio::test]
    async fn resources_are_scoped_by_namespace() {
        let storage = SqliteStorage::init(":memory:").unwrap();
        let workflow_def = WorkflowDef {
            id: "same-id".to_string(),
            tasks: vec![],
            data_bindings: vec![],
        };
        storage
            .save_workflow_def("namespace-a", workflow_def.clone())
            .await
            .unwrap();
        storage
            .save_workflow_def("namespace-b", workflow_def)
            .await
            .unwrap();
        assert!(
            storage
                .get_workflow_def("namespace-a", "same-id")
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            storage
                .get_workflow_def("namespace-c", "same-id")
                .await
                .unwrap()
                .is_none()
        );

        let function_def = FunctionDef {
            id: "same-id".to_string(),
            dependencies: vec![],
            code: "export default async function run() { return {}; }".to_string(),
        };
        storage
            .save_function_def("namespace-a", function_def.clone())
            .await
            .unwrap();
        storage
            .save_function_def("namespace-b", function_def)
            .await
            .unwrap();
        assert!(
            storage
                .get_function_def("namespace-b", "same-id")
                .await
                .unwrap()
                .is_some()
        );

        let instance = workflow_instance("same-id", WorkflowStatus::Pending);
        storage
            .save_workflow_instance("namespace-a", instance.clone())
            .await
            .unwrap();
        storage
            .save_workflow_instance("namespace-b", instance)
            .await
            .unwrap();
        assert_eq!(
            storage
                .list_workflow_instances("namespace-a")
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            storage
                .list_workflow_instances("namespace-b")
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn migrates_legacy_rows_to_default_namespace_without_changing_json() {
        let path = std::env::temp_dir().join(format!(
            "runhelm-legacy-migration-{}-{}.sqlite",
            std::process::id(),
            SqliteStorage::unix_timestamp_secs()
        ));
        let _ = std::fs::remove_file(&path);

        let workflow_def = WorkflowDef {
            id: "workflow-1".to_string(),
            tasks: vec![],
            data_bindings: vec![],
        };
        let workflow_json = serde_json::to_string(&workflow_def).unwrap();
        let instance = workflow_instance("instance-1", WorkflowStatus::Pending);
        let instance_json = serde_json::to_string(&instance).unwrap();

        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(
                    r#"
                    CREATE TABLE workflow_defs (
                        id VARCHAR(255) PRIMARY KEY NOT NULL,
                        json TEXT NOT NULL,
                        created_at BIGINT NOT NULL,
                        updated_at BIGINT NOT NULL
                    );
                    CREATE TABLE function_defs (
                        id VARCHAR(255) PRIMARY KEY NOT NULL,
                        json TEXT NOT NULL,
                        created_at BIGINT NOT NULL,
                        updated_at BIGINT NOT NULL
                    );
                    CREATE TABLE workflow_instance (
                        id VARCHAR(255) PRIMARY KEY NOT NULL,
                        json TEXT NOT NULL,
                        created_at BIGINT NOT NULL,
                        updated_at BIGINT NOT NULL
                    );
                    "#,
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO workflow_defs (id, json, created_at, updated_at) VALUES (?1, ?2, 1, 1)",
                    params!["workflow-1", workflow_json],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO workflow_instance (id, json, created_at, updated_at) VALUES (?1, ?2, 1, 1)",
                    params!["instance-1", instance_json],
                )
                .unwrap();
        }

        let storage = SqliteStorage::init(&path).unwrap();
        assert!(
            storage
                .get_workflow_def("default", "workflow-1")
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            storage
                .get_workflow_instance("default", "instance-1")
                .await
                .unwrap()
                .is_some()
        );

        let connection = Connection::open(&path).unwrap();
        let migrated_json: String = connection
            .query_row(
                "SELECT json FROM workflow_defs WHERE namespace_id = 'default' AND id = 'workflow-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            serde_json::from_str::<serde_json::Value>(&migrated_json)
                .unwrap()
                .get("namespace_id")
                .is_none()
        );

        let _ = std::fs::remove_file(&path);
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
            .save_function_def("default", function_def.clone())
            .await
            .unwrap();

        let loaded = storage
            .get_function_def("default", "function-1")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(loaded.id, function_def.id);
        assert_eq!(loaded.dependencies.len(), 1);
        assert!(
            storage
                .delete_function_def("default", "function-1")
                .await
                .unwrap()
        );
        assert!(
            !storage
                .delete_function_def("default", "function-1")
                .await
                .unwrap()
        );
        assert!(
            storage
                .get_function_def("default", "function-1")
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
            .save_workflow_instance("default", completed.clone())
            .await
            .unwrap();
        storage
            .save_workflow_instance("default", running.clone())
            .await
            .unwrap();

        let loaded = storage
            .get_workflow_instance("default", "completed-instance")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.id, completed.id);
        assert_eq!(loaded.status, WorkflowStatus::Completed);

        let mut ids: Vec<String> = storage
            .list_workflow_instances("default")
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
            .map(|item| item.instance.id)
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
        storage
            .save_workflow_instance("default", instance)
            .await
            .unwrap();

        assert_eq!(
            storage
                .get_task_result("default", "instance-1", "completed")
                .await
                .unwrap(),
            TaskResult::Success(json!({"ok": true}))
        );
        assert_eq!(
            storage
                .get_task_result("default", "instance-1", "pending")
                .await
                .unwrap(),
            TaskResult::Pending
        );
        assert_eq!(
            storage
                .get_task_result("default", "instance-1", "running")
                .await
                .unwrap(),
            TaskResult::Running
        );
        assert_eq!(
            storage
                .get_task_result("default", "instance-1", "failed")
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
