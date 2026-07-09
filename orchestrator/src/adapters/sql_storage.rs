use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use std::collections::HashMap;
use std::str::FromStr;

use crate::core::models::{FunctionDef, TaskInstance, TaskStatus};
use crate::core::util::unix_timestamp_ms;
use crate::core::workflow::events::WorkflowEventRecord;
use crate::core::workflow::models::{
    WorkerHostId, WorkflowDef, WorkflowInfo, WorkflowInstance, WorkflowStatus,
};
use crate::ports::storage::{
    StoragePort, StorageResult, WorkflowInfoCursor, WorkflowInfoListRequest, WorkflowInfoPage,
    WorkflowInstanceFilter, WorkflowVersionConflict,
};

const INITIAL_SCHEMA_MIGRATION: &str = "001_initial_sql_storage_schema";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlDialect {
    Sqlite,
    Postgres,
    Mysql,
}

impl SqlDialect {
    pub fn from_database_url(database_url: &str) -> anyhow::Result<Self> {
        if database_url.starts_with("sqlite:") {
            Ok(Self::Sqlite)
        } else if database_url.starts_with("postgres:") || database_url.starts_with("postgresql:") {
            Ok(Self::Postgres)
        } else if database_url.starts_with("mysql:") || database_url.starts_with("mariadb:") {
            Ok(Self::Mysql)
        } else {
            anyhow::bail!("unsupported SQL storage database URL scheme");
        }
    }
}

pub struct SqlStorage {
    pool: SqlitePool,
}

impl SqlStorage {
    pub async fn connect(database_url: &str) -> anyhow::Result<Self> {
        match SqlDialect::from_database_url(database_url)? {
            SqlDialect::Sqlite => Self::connect_sqlite(database_url).await,
            SqlDialect::Postgres | SqlDialect::Mysql => {
                anyhow::bail!("only sqlite SQL storage is implemented")
            }
        }
    }

    pub async fn connect_sqlite(database_url: &str) -> anyhow::Result<Self> {
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        let storage = Self { pool };
        storage.run_migrations().await?;
        Ok(storage)
    }

    async fn run_migrations(&self) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version TEXT PRIMARY KEY,
                applied_at_epoch_ms INTEGER NOT NULL
            )",
        )
        .execute(&mut *tx)
        .await?;

        let applied = sqlx::query("SELECT version FROM schema_migrations WHERE version = ?")
            .bind(INITIAL_SCHEMA_MIGRATION)
            .fetch_optional(&mut *tx)
            .await?;

        if applied.is_none() {
            for statement in INITIAL_SCHEMA_SQL {
                sqlx::query(statement).execute(&mut *tx).await?;
            }
            sqlx::query(
                "INSERT INTO schema_migrations (version, applied_at_epoch_ms) VALUES (?, ?)",
            )
            .bind(INITIAL_SCHEMA_MIGRATION)
            .bind(i64_from_u64(unix_timestamp_ms()?)?)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}

#[async_trait]
impl StoragePort for SqlStorage {
    async fn save_workflow_def(&self, def: WorkflowDef) -> StorageResult<()> {
        let now = i64_from_u64(unix_timestamp_ms()?)?;
        let definition_json = serde_json::to_string(&def)?;
        sqlx::query(
            "INSERT INTO workflow_defs (id, definition_json, created_at_epoch_ms, updated_at_epoch_ms)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                definition_json = excluded.definition_json,
                updated_at_epoch_ms = excluded.updated_at_epoch_ms",
        )
        .bind(&def.id)
        .bind(definition_json)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_workflow_def(&self, id: &str) -> StorageResult<Option<WorkflowDef>> {
        let row = sqlx::query("SELECT definition_json FROM workflow_defs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row
            .map(|row| deserialize_json(row.get::<String, _>("definition_json").as_str()))
            .transpose()?)
    }

    async fn save_function_def(&self, def: FunctionDef) -> StorageResult<()> {
        let now = i64_from_u64(unix_timestamp_ms()?)?;
        let definition_json = serde_json::to_string(&def)?;
        sqlx::query(
            "INSERT INTO function_defs (id, definition_json, created_at_epoch_ms, updated_at_epoch_ms)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                definition_json = excluded.definition_json,
                updated_at_epoch_ms = excluded.updated_at_epoch_ms",
        )
        .bind(&def.id)
        .bind(definition_json)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_function_def(&self, id: &str) -> StorageResult<Option<FunctionDef>> {
        let row = sqlx::query("SELECT definition_json FROM function_defs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row
            .map(|row| deserialize_json(row.get::<String, _>("definition_json").as_str()))
            .transpose()?)
    }

    async fn delete_function_def(&self, id: &str) -> StorageResult<bool> {
        let result = sqlx::query("DELETE FROM function_defs WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn get_workflow_instance(&self, id: &str) -> StorageResult<Option<WorkflowInstance>> {
        let Some(row) = sqlx::query(
            "SELECT id, workflow_def_id, version, status, trigger_input_json, pinned_worker_host_id
             FROM workflow_instances
             WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        let task_rows = sqlx::query(
            "SELECT task_attempt_id, task_def_id, status_json, satisfaction_status, generation_index,
                    human_input_json, input_data_json, input_mapping_json, output_data_json,
                    verifier_metadata_json
             FROM workflow_tasks
             WHERE workflow_instance_id = ?",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        let mut tasks = HashMap::new();
        for row in task_rows {
            let task_attempt_id = row.get::<String, _>("task_attempt_id");
            let task = TaskInstance {
                task_def_id: row.get("task_def_id"),
                status: deserialize_json(&row.get::<String, _>("status_json"))?,
                satisfaction_status: deserialize_json(
                    &row.get::<String, _>("satisfaction_status"),
                )?,
                human_input: optional_json(row.get::<Option<String>, _>("human_input_json"))?,
                input_data: deserialize_json(&row.get::<String, _>("input_data_json"))?,
                input_mapping: deserialize_json(&row.get::<String, _>("input_mapping_json"))?,
                output_data: optional_json(row.get::<Option<String>, _>("output_data_json"))?,
                generation_index: u32_from_i64(row.get::<i64, _>("generation_index"))?,
                verifier_metadata: optional_json(
                    row.get::<Option<String>, _>("verifier_metadata_json"),
                )?,
            };
            tasks.insert(task_attempt_id, task);
        }

        let verifier_rows = sqlx::query(
            "SELECT verifier_task_id, state_json
             FROM workflow_verifier_states
             WHERE workflow_instance_id = ?",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        let mut verifier_states = HashMap::new();
        for row in verifier_rows {
            verifier_states.insert(
                row.get::<String, _>("verifier_task_id"),
                deserialize_json(&row.get::<String, _>("state_json"))?,
            );
        }

        Ok(Some(WorkflowInstance {
            id: row.get("id"),
            workflow_def_id: row.get("workflow_def_id"),
            version: u64_from_i64(row.get::<i64, _>("version"))?,
            status: workflow_status_from_name(&row.get::<String, _>("status"))?,
            trigger_input: optional_json(row.get::<Option<String>, _>("trigger_input_json"))?,
            pinned_worker_host: row
                .get::<Option<String>, _>("pinned_worker_host_id")
                .map(WorkerHostId),
            tasks,
            verifier_states,
        }))
    }

    async fn get_workflow_instance_events(
        &self,
        workflow_instance_id: &str,
    ) -> StorageResult<Vec<WorkflowEventRecord>> {
        let rows = sqlx::query(
            "SELECT created_at_epoch_ms, event_json
             FROM workflow_events
             WHERE workflow_instance_id = ?
             ORDER BY event_sequence ASC",
        )
        .bind(workflow_instance_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(WorkflowEventRecord {
                    created_time: u64_from_i64(row.get::<i64, _>("created_at_epoch_ms"))?,
                    event: deserialize_json(&row.get::<String, _>("event_json"))?,
                })
            })
            .collect()
    }

    async fn list_workflow_info(
        &self,
        request: WorkflowInfoListRequest,
    ) -> StorageResult<WorkflowInfoPage> {
        if request
            .filters
            .iter()
            .any(|filter| matches!(filter, WorkflowInstanceFilter::Statuses(statuses) if statuses.is_empty()))
        {
            return Ok(WorkflowInfoPage {
                workflows: vec![],
                next_cursor: None,
            });
        }

        let mut conditions = Vec::new();
        for filter in &request.filters {
            match filter {
                WorkflowInstanceFilter::Statuses(statuses) => {
                    let placeholders = vec!["?"; statuses.len()].join(", ");
                    conditions.push(format!("wi.status IN ({placeholders})"));
                }
                WorkflowInstanceFilter::WorkflowDefId(_) => {
                    conditions.push("wi.workflow_def_id = ?".to_string());
                }
            }
        }

        if request.page.cursor.is_some() {
            conditions.push(
                "(wi.modified_at_epoch_ms < ? OR (wi.modified_at_epoch_ms = ? AND wi.id < ?))"
                    .to_string(),
            );
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT
                wi.id,
                wi.workflow_def_id,
                wi.status,
                wi.created_at_epoch_ms,
                wi.modified_at_epoch_ms,
                wi.completed_at_epoch_ms,
                COUNT(wt.task_attempt_id) AS total_task_count,
                COALESCE(SUM(CASE WHEN wt.status = 'completed' THEN 1 ELSE 0 END), 0) AS completed_task_count
             FROM workflow_instances wi
             LEFT JOIN workflow_tasks wt ON wt.workflow_instance_id = wi.id
             {where_clause}
             GROUP BY wi.id, wi.workflow_def_id, wi.status, wi.created_at_epoch_ms,
                      wi.modified_at_epoch_ms, wi.completed_at_epoch_ms
             ORDER BY wi.modified_at_epoch_ms DESC, wi.id DESC
             LIMIT ?"
        );

        let mut query = sqlx::query(&sql);

        for filter in &request.filters {
            match filter {
                WorkflowInstanceFilter::Statuses(statuses) => {
                    for status in statuses {
                        query = query.bind(workflow_status_name(status));
                    }
                }
                WorkflowInstanceFilter::WorkflowDefId(workflow_def_id) => {
                    query = query.bind(workflow_def_id);
                }
            }
        }

        if let Some(cursor) = &request.page.cursor {
            query = query
                .bind(i64_from_u64(cursor.modified_at_epoch_ms)?)
                .bind(i64_from_u64(cursor.modified_at_epoch_ms)?)
                .bind(&cursor.workflow_instance_id);
        }

        query = query.bind(i64_from_usize(request.page.limit + 1)?);

        let rows = query.fetch_all(&self.pool).await?;
        let has_more = rows.len() > request.page.limit;
        let workflows: Vec<WorkflowInfo> = rows
            .into_iter()
            .take(request.page.limit)
            .map(workflow_info_from_row)
            .collect::<anyhow::Result<Vec<_>>>()?;

        let next_cursor =
            has_more
                .then(|| workflows.last())
                .flatten()
                .map(|info| WorkflowInfoCursor {
                    modified_at_epoch_ms: info.modified_at_epoch_ms,
                    workflow_instance_id: info.id.clone(),
                });

        Ok(WorkflowInfoPage {
            workflows,
            next_cursor,
        })
    }

    async fn save_workflow_instance(
        &self,
        expected_version: u64,
        events: Vec<WorkflowEventRecord>,
        instance: WorkflowInstance,
    ) -> StorageResult<()> {
        let mut tx = self.pool.begin().await?;
        let workflow_instance_id = instance.id.clone();

        let existing = sqlx::query(
            "SELECT version, created_at_epoch_ms, completed_at_epoch_ms
             FROM workflow_instances
             WHERE id = ?",
        )
        .bind(&workflow_instance_id)
        .fetch_optional(&mut *tx)
        .await?;

        let actual_version = existing
            .as_ref()
            .map(|row| u64_from_i64(row.get::<i64, _>("version")))
            .transpose()?
            .unwrap_or(0);

        if actual_version != expected_version {
            return Err(WorkflowVersionConflict {
                workflow_instance_id,
                expected_version,
                actual_version,
            }
            .into());
        }

        let created_from_events_at_epoch_ms = events
            .first()
            .map(|event| event.created_time)
            .unwrap_or(unix_timestamp_ms()?);

        let modified_at_epoch_ms = events
            .last()
            .map(|event| event.created_time)
            .unwrap_or(created_from_events_at_epoch_ms);

        let created_at_epoch_ms = existing
            .as_ref()
            .map(|row| u64_from_i64(row.get::<i64, _>("created_at_epoch_ms")))
            .transpose()?
            .unwrap_or(created_from_events_at_epoch_ms);

        let completed_at_epoch_ms = existing
            .as_ref()
            .and_then(|row| row.get::<Option<i64>, _>("completed_at_epoch_ms"))
            .map(u64_from_i64)
            .transpose()?
            .or_else(|| workflow_completed_at(&instance, modified_at_epoch_ms));

        insert_events(&mut tx, expected_version, &instance.id, &events).await?;

        upsert_workflow_instance(
            &mut tx,
            &instance,
            created_at_epoch_ms,
            modified_at_epoch_ms,
            completed_at_epoch_ms,
        )
        .await?;
        replace_tasks(&mut tx, &instance).await?;
        replace_verifier_states(&mut tx, &instance).await?;

        tx.commit().await?;
        Ok(())
    }
}

async fn insert_events(
    tx: &mut Transaction<'_, Sqlite>,
    expected_version: u64,
    workflow_instance_id: &str,
    events: &[WorkflowEventRecord],
) -> anyhow::Result<()> {
    for (index, event) in events.iter().enumerate() {
        sqlx::query(
            "INSERT INTO workflow_events (
                workflow_instance_id, event_sequence, created_at_epoch_ms, event_json
             )
             VALUES (?, ?, ?, ?)",
        )
        .bind(workflow_instance_id)
        .bind(i64_from_u64(expected_version + index as u64 + 1)?)
        .bind(i64_from_u64(event.created_time)?)
        .bind(serde_json::to_string(&event.event)?)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn upsert_workflow_instance(
    tx: &mut Transaction<'_, Sqlite>,
    instance: &WorkflowInstance,
    created_at_epoch_ms: u64,
    modified_at_epoch_ms: u64,
    completed_at_epoch_ms: Option<u64>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO workflow_instances (
            id, workflow_def_id, version, status, trigger_input_json, pinned_worker_host_id,
            created_at_epoch_ms, modified_at_epoch_ms, completed_at_epoch_ms
         )
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            workflow_def_id = excluded.workflow_def_id,
            version = excluded.version,
            status = excluded.status,
            trigger_input_json = excluded.trigger_input_json,
            pinned_worker_host_id = excluded.pinned_worker_host_id,
            modified_at_epoch_ms = excluded.modified_at_epoch_ms,
            completed_at_epoch_ms = excluded.completed_at_epoch_ms",
    )
    .bind(&instance.id)
    .bind(&instance.workflow_def_id)
    .bind(i64_from_u64(instance.version)?)
    .bind(workflow_status_name(&instance.status))
    .bind(optional_json_string(&instance.trigger_input)?)
    .bind(
        instance
            .pinned_worker_host
            .as_ref()
            .map(|host| host.0.as_str()),
    )
    .bind(i64_from_u64(created_at_epoch_ms)?)
    .bind(i64_from_u64(modified_at_epoch_ms)?)
    .bind(optional_i64_from_u64(completed_at_epoch_ms)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn replace_tasks(
    tx: &mut Transaction<'_, Sqlite>,
    instance: &WorkflowInstance,
) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM workflow_tasks WHERE workflow_instance_id = ?")
        .bind(&instance.id)
        .execute(&mut **tx)
        .await?;

    for (task_attempt_id, task) in &instance.tasks {
        sqlx::query(
            "INSERT INTO workflow_tasks (
                workflow_instance_id, task_attempt_id, task_def_id, status, status_json,
                satisfaction_status, generation_index, human_input_json, input_data_json,
                input_mapping_json, output_data_json, verifier_metadata_json
             )
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&instance.id)
        .bind(task_attempt_id)
        .bind(&task.task_def_id)
        .bind(task_status_name(&task.status))
        .bind(serde_json::to_string(&task.status)?)
        .bind(serde_json::to_string(&task.satisfaction_status)?)
        .bind(i64::from(task.generation_index))
        .bind(optional_json_string(&task.human_input)?)
        .bind(serde_json::to_string(&task.input_data)?)
        .bind(serde_json::to_string(&task.input_mapping)?)
        .bind(optional_json_string(&task.output_data)?)
        .bind(optional_json_string(&task.verifier_metadata)?)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn replace_verifier_states(
    tx: &mut Transaction<'_, Sqlite>,
    instance: &WorkflowInstance,
) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM workflow_verifier_states WHERE workflow_instance_id = ?")
        .bind(&instance.id)
        .execute(&mut **tx)
        .await?;

    for (verifier_task_id, state) in &instance.verifier_states {
        sqlx::query(
            "INSERT INTO workflow_verifier_states (
                workflow_instance_id, verifier_task_id, state_json
             )
             VALUES (?, ?, ?)",
        )
        .bind(&instance.id)
        .bind(verifier_task_id)
        .bind(serde_json::to_string(state)?)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn workflow_info_from_row(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<WorkflowInfo> {
    Ok(WorkflowInfo {
        id: row.get("id"),
        workflow_def_id: row.get("workflow_def_id"),
        created_at_epoch_ms: row
            .get::<Option<i64>, _>("created_at_epoch_ms")
            .map(u64_from_i64)
            .transpose()?,
        modified_at_epoch_ms: u64_from_i64(row.get::<i64, _>("modified_at_epoch_ms"))?,
        completed_at_epoch_ms: row
            .get::<Option<i64>, _>("completed_at_epoch_ms")
            .map(u64_from_i64)
            .transpose()?,
        status: workflow_status_from_name(&row.get::<String, _>("status"))?,
        total_task_count: usize_from_i64(row.get::<i64, _>("total_task_count"))?,
        completed_task_count: usize_from_i64(row.get::<i64, _>("completed_task_count"))?,
    })
}

fn workflow_completed_at(instance: &WorkflowInstance, modified_at_epoch_ms: u64) -> Option<u64> {
    matches!(
        instance.status,
        WorkflowStatus::Completed | WorkflowStatus::Failed
    )
    .then_some(modified_at_epoch_ms)
}

fn workflow_status_name(status: &WorkflowStatus) -> &'static str {
    match status {
        WorkflowStatus::Pending => "pending",
        WorkflowStatus::Running => "running",
        WorkflowStatus::Paused => "paused",
        WorkflowStatus::InputNeeded => "input_needed",
        WorkflowStatus::Completed => "completed",
        WorkflowStatus::Failed => "failed",
    }
}

fn workflow_status_from_name(value: &str) -> anyhow::Result<WorkflowStatus> {
    match value {
        "pending" => Ok(WorkflowStatus::Pending),
        "running" => Ok(WorkflowStatus::Running),
        "paused" => Ok(WorkflowStatus::Paused),
        "input_needed" => Ok(WorkflowStatus::InputNeeded),
        "completed" => Ok(WorkflowStatus::Completed),
        "failed" => Ok(WorkflowStatus::Failed),
        _ => anyhow::bail!("unknown workflow status {value}"),
    }
}

fn task_status_name(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Running => "running",
        TaskStatus::InputNeeded { .. } => "input_needed",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
    }
}

fn optional_json_string<T>(value: &Option<T>) -> anyhow::Result<Option<String>>
where
    T: serde::Serialize,
{
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(Into::into)
}

fn optional_json<T>(value: Option<String>) -> anyhow::Result<Option<T>>
where
    T: serde::de::DeserializeOwned,
{
    value.as_deref().map(deserialize_json).transpose()
}

fn deserialize_json<T>(value: &str) -> anyhow::Result<T>
where
    T: serde::de::DeserializeOwned,
{
    Ok(serde_json::from_str(value)?)
}

fn i64_from_u64(value: u64) -> anyhow::Result<i64> {
    i64::try_from(value).map_err(Into::into)
}

fn optional_i64_from_u64(value: Option<u64>) -> anyhow::Result<Option<i64>> {
    value.map(i64_from_u64).transpose()
}

fn u64_from_i64(value: i64) -> anyhow::Result<u64> {
    u64::try_from(value).map_err(Into::into)
}

fn u32_from_i64(value: i64) -> anyhow::Result<u32> {
    u32::try_from(value).map_err(Into::into)
}

fn usize_from_i64(value: i64) -> anyhow::Result<usize> {
    usize::try_from(value).map_err(Into::into)
}

fn i64_from_usize(value: usize) -> anyhow::Result<i64> {
    i64::try_from(value).map_err(Into::into)
}

const INITIAL_SCHEMA_SQL: &[&str] = &[
    "CREATE TABLE workflow_defs (
        id TEXT PRIMARY KEY,
        definition_json TEXT NOT NULL,
        created_at_epoch_ms INTEGER NOT NULL,
        updated_at_epoch_ms INTEGER NOT NULL
    )",
    "CREATE TABLE function_defs (
        id TEXT PRIMARY KEY,
        definition_json TEXT NOT NULL,
        created_at_epoch_ms INTEGER NOT NULL,
        updated_at_epoch_ms INTEGER NOT NULL
    )",
    "CREATE TABLE workflow_instances (
        id TEXT PRIMARY KEY,
        workflow_def_id TEXT NOT NULL,
        version INTEGER NOT NULL,
        status TEXT NOT NULL,
        trigger_input_json TEXT,
        pinned_worker_host_id TEXT,
        created_at_epoch_ms INTEGER NOT NULL,
        modified_at_epoch_ms INTEGER NOT NULL,
        completed_at_epoch_ms INTEGER
    )",
    "CREATE TABLE workflow_tasks (
        workflow_instance_id TEXT NOT NULL,
        task_attempt_id TEXT NOT NULL,
        task_def_id TEXT NOT NULL,
        status TEXT NOT NULL,
        status_json TEXT NOT NULL,
        satisfaction_status TEXT NOT NULL,
        generation_index INTEGER NOT NULL,
        human_input_json TEXT,
        input_data_json TEXT NOT NULL,
        input_mapping_json TEXT NOT NULL,
        output_data_json TEXT,
        verifier_metadata_json TEXT,
        PRIMARY KEY (workflow_instance_id, task_attempt_id)
    )",
    "CREATE TABLE workflow_verifier_states (
        workflow_instance_id TEXT NOT NULL,
        verifier_task_id TEXT NOT NULL,
        state_json TEXT NOT NULL,
        PRIMARY KEY (workflow_instance_id, verifier_task_id)
    )",
    "CREATE TABLE workflow_events (
        workflow_instance_id TEXT NOT NULL,
        event_sequence INTEGER NOT NULL,
        created_at_epoch_ms INTEGER NOT NULL,
        event_json TEXT NOT NULL,
        PRIMARY KEY (workflow_instance_id, event_sequence)
    )",
    "CREATE INDEX workflow_instances_workflow_def_idx ON workflow_instances (workflow_def_id)",
    "CREATE INDEX workflow_instances_status_idx ON workflow_instances (status)",
    "CREATE INDEX workflow_instances_modified_idx ON workflow_instances (modified_at_epoch_ms, id)",
    "CREATE INDEX workflow_instances_status_modified_idx ON workflow_instances (status, modified_at_epoch_ms, id)",
    "CREATE INDEX workflow_instances_workflow_def_modified_idx ON workflow_instances (workflow_def_id, modified_at_epoch_ms, id)",
    "CREATE INDEX workflow_tasks_instance_status_idx ON workflow_tasks (workflow_instance_id, status)",
    "CREATE INDEX workflow_tasks_instance_task_def_idx ON workflow_tasks (workflow_instance_id, task_def_id)",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{TaskInputMapping, TaskSatisfactionStatus, VerifierAttemptMetadata};
    use crate::core::workflow::events::WorkflowInstanceEvent;
    use crate::core::workflow::models::{
        VerifierFeedbackEntry, VerifierGenerationState, VerifierStateStatus,
    };
    use crate::ports::storage::{StorageError, WorkflowInfoPageRequest, WorkflowInstanceFilter};
    use serde_json::json;

    async fn storage() -> SqlStorage {
        SqlStorage::connect("sqlite::memory:").await.unwrap()
    }

    fn instance(id: &str, status: WorkflowStatus) -> WorkflowInstance {
        WorkflowInstance {
            id: id.to_string(),
            workflow_def_id: "wf".to_string(),
            version: 0,
            status,
            trigger_input: Some(json!({"request": true})),
            pinned_worker_host: Some(WorkerHostId("host-a".to_string())),
            tasks: HashMap::new(),
            verifier_states: HashMap::new(),
        }
    }

    fn task(status: TaskStatus) -> TaskInstance {
        TaskInstance {
            task_def_id: "task-a".to_string(),
            status,
            satisfaction_status: TaskSatisfactionStatus::Satisfied,
            human_input: Some(json!("approved")),
            input_data: vec![json!({"input": 1})],
            input_mapping: vec![TaskInputMapping {
                task_id: "source".to_string(),
                generation: 1,
            }],
            output_data: Some(json!({"output": 2})),
            generation_index: 2,
            verifier_metadata: Some(VerifierAttemptMetadata {
                status: crate::core::models::VerifierAttemptStatus::Accepted,
                decision: Some(crate::core::models::VerifierDecision::Complete),
                feedback: Some("ok".to_string()),
                verifier_output: Some(json!({"decision": "complete"})),
                exit_reason: None,
            }),
        }
    }

    fn event_record(created_time: u64) -> WorkflowEventRecord {
        WorkflowEventRecord {
            created_time,
            event: WorkflowInstanceEvent::WorkflowStatusChanged {
                status: WorkflowStatus::Running,
            },
        }
    }

    fn list_request(filters: Vec<WorkflowInstanceFilter>) -> WorkflowInfoListRequest {
        WorkflowInfoListRequest {
            filters,
            page: WorkflowInfoPageRequest {
                limit: 100,
                cursor: None,
            },
        }
    }

    #[tokio::test]
    async fn reconstructs_workflow_instance_from_normalized_rows() {
        let storage = storage().await;
        let mut instance = instance("wf-1", WorkflowStatus::InputNeeded);
        instance.tasks.insert(
            "task-a[2]".to_string(),
            task(TaskStatus::InputNeeded {
                input_request: "need approval".to_string(),
            }),
        );
        instance.verifier_states.insert(
            "verify".to_string(),
            VerifierGenerationState {
                verifier_task_id: "verify".to_string(),
                rerun_start_task_id: "task-a".to_string(),
                latest_generation: 2,
                selected_generation: Some(2),
                feedback_history: vec![VerifierFeedbackEntry {
                    generation_index: 1,
                    feedback: "again".to_string(),
                    verifier_output: json!({"decision": "continue"}),
                }],
                status: VerifierStateStatus::Accepted,
                exit_reason: None,
            },
        );

        storage
            .save_workflow_instance(0, vec![event_record(1000)], instance.clone())
            .await
            .unwrap();

        let saved = storage
            .get_workflow_instance("wf-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.id, instance.id);
        assert_eq!(saved.status, WorkflowStatus::InputNeeded);
        assert_eq!(saved.pinned_worker_host, instance.pinned_worker_host);
        assert_eq!(
            saved.tasks["task-a[2]"].status,
            TaskStatus::InputNeeded {
                input_request: "need approval".to_string()
            }
        );
        assert_eq!(
            saved.tasks["task-a[2]"].input_data,
            vec![json!({"input": 1})]
        );
        assert_eq!(
            saved.verifier_states["verify"].feedback_history[0].feedback,
            "again"
        );
    }

    #[tokio::test]
    async fn commits_events_workflow_tasks_verifiers_and_summary() {
        let storage = storage().await;
        let mut instance = instance("wf-1", WorkflowStatus::Running);
        instance
            .tasks
            .insert("task-a[1]".to_string(), task(TaskStatus::Completed));
        storage
            .save_workflow_instance(0, vec![event_record(1000)], instance.clone())
            .await
            .unwrap();

        let events = storage.get_workflow_instance_events("wf-1").await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].created_time, 1000);

        let infos = storage
            .list_workflow_info(list_request(vec![]))
            .await
            .unwrap()
            .workflows;
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].total_task_count, 1);
        assert_eq!(infos[0].completed_task_count, 1);
    }

    #[tokio::test]
    async fn version_conflict_does_not_partially_write() {
        let storage = storage().await;
        let mut instance = instance("wf-1", WorkflowStatus::Pending);
        instance.version = 1;
        storage
            .save_workflow_instance(0, vec![event_record(1000)], instance.clone())
            .await
            .unwrap();

        instance.status = WorkflowStatus::Completed;
        instance.version = 2;
        instance
            .tasks
            .insert("task-a[1]".to_string(), task(TaskStatus::Completed));
        let error = storage
            .save_workflow_instance(0, vec![event_record(2000)], instance)
            .await
            .unwrap_err();
        let StorageError::WorkflowVersionConflict(conflict) = error else {
            panic!("expected version conflict");
        };
        assert_eq!(conflict.actual_version, 1);

        let saved = storage
            .get_workflow_instance("wf-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.status, WorkflowStatus::Pending);
        assert!(saved.tasks.is_empty());
        assert_eq!(
            storage
                .get_workflow_instance_events("wf-1")
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn list_workflow_info_filters_and_paginates() {
        let storage = storage().await;
        storage
            .save_workflow_instance(
                0,
                vec![event_record(3000)],
                instance("newest", WorkflowStatus::Pending),
            )
            .await
            .unwrap();
        storage
            .save_workflow_instance(
                0,
                vec![event_record(2000)],
                instance("middle", WorkflowStatus::Running),
            )
            .await
            .unwrap();
        storage
            .save_workflow_instance(
                0,
                vec![event_record(1000)],
                instance("oldest", WorkflowStatus::Completed),
            )
            .await
            .unwrap();

        let active = storage
            .list_workflow_info(list_request(vec![WorkflowInstanceFilter::Statuses(vec![
                WorkflowStatus::Pending,
                WorkflowStatus::Running,
            ])]))
            .await
            .unwrap()
            .workflows;
        let ids: Vec<&str> = active.iter().map(|info| info.id.as_str()).collect();
        assert_eq!(ids, vec!["newest", "middle"]);

        let first_page = storage
            .list_workflow_info(WorkflowInfoListRequest {
                filters: vec![],
                page: WorkflowInfoPageRequest {
                    limit: 1,
                    cursor: None,
                },
            })
            .await
            .unwrap();
        assert_eq!(first_page.workflows[0].id, "newest");

        let second_page = storage
            .list_workflow_info(WorkflowInfoListRequest {
                filters: vec![],
                page: WorkflowInfoPageRequest {
                    limit: 2,
                    cursor: first_page.next_cursor,
                },
            })
            .await
            .unwrap();
        let ids: Vec<&str> = second_page
            .workflows
            .iter()
            .map(|info| info.id.as_str())
            .collect();
        assert_eq!(ids, vec!["middle", "oldest"]);
    }
}
