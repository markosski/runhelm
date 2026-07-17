use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use aws_sdk_dynamodb::types::{AttributeValue, Delete, Put, TransactWriteItem};
use aws_sdk_s3::primitives::ByteStream;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use tracing::warn;

use crate::core::function::models::FunctionDef;
use crate::core::task::{TaskInstance, TaskStatus};
use crate::core::util::unix_timestamp_ms;
use crate::core::workflow::events::{WorkflowEventRecord, changed_task_attempt_ids};
use crate::core::workflow::models::{
    WorkflowDef, WorkflowDefSummary, WorkflowInfo, WorkflowInstance, WorkflowStatus,
};
use crate::ports::storage::{
    StorageError, StoragePort, StorageResult, WorkflowEventPage, WorkflowEventPageRequest,
    WorkflowInfoCursor, WorkflowInfoPage, WorkflowInfoPageRequest, WorkflowInstanceFilter,
    WorkflowVersionConflict,
};

const PK: &str = "pk";
const SK: &str = "sk";
const META_SK: &str = "META";
const WORKFLOW_DEFINITION_PK: &str = "WORKFLOW";
const FUNCTION_DEFINITION_PK: &str = "FUNCTION";
const LIST_SHARD_COUNT: u8 = 16;
const MAX_TRANSACTION_ITEMS: usize = 100;

#[derive(Debug, Clone)]
pub struct AwsStorageConfig {
    pub definitions_table: String,
    pub workflow_instances_table: String,
    pub workflow_events_table: String,
    pub tasks_table: String,
    pub bucket: String,
    pub prefix: String,
    pub region: Option<String>,
    pub endpoint_url: Option<String>,
}

impl AwsStorageConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let required = |name: &str| {
            std::env::var(name)
                .map_err(|_| anyhow::anyhow!("{name} is required when RUNHELM_STORAGE=aws"))
        };
        Ok(Self {
            definitions_table: required("RUNHELM_AWS_DEFINITIONS_TABLE")?,
            workflow_instances_table: required("RUNHELM_AWS_WORKFLOW_INSTANCES_TABLE")?,
            workflow_events_table: required("RUNHELM_AWS_WORKFLOW_EVENTS_TABLE")?,
            tasks_table: required("RUNHELM_AWS_TASKS_TABLE")?,
            bucket: required("RUNHELM_AWS_S3_BUCKET")?,
            prefix: std::env::var("RUNHELM_AWS_S3_PREFIX")
                .unwrap_or_else(|_| "runhelm".to_string())
                .trim_matches('/')
                .to_string(),
            region: std::env::var("RUNHELM_AWS_REGION")
                .ok()
                .or_else(|| std::env::var("AWS_REGION").ok()),
            endpoint_url: std::env::var("RUNHELM_AWS_ENDPOINT_URL").ok(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Table {
    Definitions,
    WorkflowInstances,
    WorkflowEvents,
    Tasks,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Record {
    pk: String,
    sk: String,
    fields: BTreeMap<String, String>,
}

impl Record {
    fn new(pk: impl Into<String>, sk: impl Into<String>) -> Self {
        Self {
            pk: pk.into(),
            sk: sk.into(),
            fields: BTreeMap::new(),
        }
    }

    fn field(mut self, name: impl Into<String>, value: impl ToString) -> Self {
        self.fields.insert(name.into(), value.to_string());
        self
    }

    fn optional_field(mut self, name: &str, value: Option<impl ToString>) -> Self {
        if let Some(value) = value {
            self.fields.insert(name.to_string(), value.to_string());
        }
        self
    }

    fn required(&self, name: &str) -> anyhow::Result<&str> {
        self.fields
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| anyhow::anyhow!("AWS storage record is missing {name}"))
    }

    fn optional(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }

    fn into_item(self) -> HashMap<String, AttributeValue> {
        let mut item = HashMap::from([
            (PK.to_string(), AttributeValue::S(self.pk)),
            (SK.to_string(), AttributeValue::S(self.sk)),
        ]);
        item.extend(
            self.fields
                .into_iter()
                .map(|(key, value)| (key, AttributeValue::S(value))),
        );
        item
    }

    fn from_item(mut item: HashMap<String, AttributeValue>) -> anyhow::Result<Self> {
        let string = |value: AttributeValue| match value {
            AttributeValue::S(value) => Ok(value),
            _ => anyhow::bail!("AWS storage expected a string attribute"),
        };
        let pk = string(
            item.remove(PK)
                .ok_or_else(|| anyhow::anyhow!("AWS storage record is missing {PK}"))?,
        )?;
        let sk = string(
            item.remove(SK)
                .ok_or_else(|| anyhow::anyhow!("AWS storage record is missing {SK}"))?,
        )?;
        let fields = item
            .into_iter()
            .map(|(key, value)| Ok((key, string(value)?)))
            .collect::<anyhow::Result<_>>()?;
        Ok(Self { pk, sk, fields })
    }
}

#[derive(Debug, Clone)]
enum DynamoWrite {
    Put {
        table: Table,
        record: Record,
    },
    Delete {
        table: Table,
        pk: String,
        sk: String,
    },
}

#[derive(Debug, Clone)]
struct QueryPage {
    records: Vec<Record>,
    has_more: bool,
}

#[derive(Debug, Clone)]
struct QueryRequest {
    pk: String,
    after_sk: Option<String>,
    before_sk: Option<String>,
    descending: bool,
    limit: usize,
}

#[derive(Debug)]
enum CommitError {
    Conflict(u64),
    Backend(anyhow::Error),
}

#[async_trait]
trait DynamoStore: Send + Sync {
    async fn get(&self, table: Table, pk: &str, sk: &str) -> anyhow::Result<Option<Record>>;
    async fn put(&self, table: Table, record: Record) -> anyhow::Result<()>;
    async fn delete(&self, table: Table, pk: &str, sk: &str) -> anyhow::Result<bool>;
    async fn query_page(&self, table: Table, request: QueryRequest) -> anyhow::Result<QueryPage>;
    async fn query_all(&self, table: Table, pk: &str) -> anyhow::Result<Vec<Record>> {
        let mut records = Vec::new();
        let mut after_sk = None;
        loop {
            let page = self
                .query_page(
                    table,
                    QueryRequest {
                        pk: pk.to_string(),
                        after_sk,
                        before_sk: None,
                        descending: false,
                        limit: 1000,
                    },
                )
                .await?;
            let next = page.records.last().map(|record| record.sk.clone());
            records.extend(page.records);
            if !page.has_more {
                break;
            }
            after_sk = next;
        }
        Ok(records)
    }
    async fn commit_workflow(
        &self,
        workflow_instance_id: &str,
        expected_version: u64,
        writes: Vec<DynamoWrite>,
    ) -> Result<(), CommitError>;
    async fn update_last_invoked(
        &self,
        workflow_def_id: &str,
        created_at_epoch_ms: u64,
    ) -> anyhow::Result<()>;
}

#[async_trait]
trait ObjectStore: Send + Sync {
    async fn put(&self, key: &str, body: Vec<u8>) -> anyhow::Result<()>;
    async fn get(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>>;
}

struct AwsDynamoStore {
    client: aws_sdk_dynamodb::Client,
    table_names: HashMap<Table, String>,
}

impl AwsDynamoStore {
    fn table_name(&self, table: Table) -> &str {
        &self.table_names[&table]
    }

    fn key(pk: &str, sk: &str) -> HashMap<String, AttributeValue> {
        HashMap::from([
            (PK.to_string(), AttributeValue::S(pk.to_string())),
            (SK.to_string(), AttributeValue::S(sk.to_string())),
        ])
    }
}

#[async_trait]
impl DynamoStore for AwsDynamoStore {
    async fn get(&self, table: Table, pk: &str, sk: &str) -> anyhow::Result<Option<Record>> {
        self.client
            .get_item()
            .table_name(self.table_name(table))
            .set_key(Some(Self::key(pk, sk)))
            .consistent_read(true)
            .send()
            .await?
            .item
            .map(Record::from_item)
            .transpose()
    }

    async fn put(&self, table: Table, record: Record) -> anyhow::Result<()> {
        self.client
            .put_item()
            .table_name(self.table_name(table))
            .set_item(Some(record.into_item()))
            .send()
            .await?;
        Ok(())
    }

    async fn delete(&self, table: Table, pk: &str, sk: &str) -> anyhow::Result<bool> {
        let output = self
            .client
            .delete_item()
            .table_name(self.table_name(table))
            .set_key(Some(Self::key(pk, sk)))
            .return_values(aws_sdk_dynamodb::types::ReturnValue::AllOld)
            .send()
            .await?;
        Ok(output.attributes.is_some())
    }

    async fn query_page(&self, table: Table, request: QueryRequest) -> anyhow::Result<QueryPage> {
        if request.limit == 0 {
            return Ok(QueryPage {
                records: vec![],
                has_more: false,
            });
        }

        let mut condition = "#pk = :pk".to_string();
        let mut builder = self
            .client
            .query()
            .table_name(self.table_name(table))
            .expression_attribute_names("#pk", PK)
            .expression_attribute_values(":pk", AttributeValue::S(request.pk.clone()))
            .scan_index_forward(!request.descending)
            .limit(i32::try_from(request.limit + 1)?);

        if let Some(after_sk) = &request.after_sk {
            condition.push_str(" AND #sk > :after_sk");
            builder = builder
                .expression_attribute_names("#sk", SK)
                .expression_attribute_values(":after_sk", AttributeValue::S(after_sk.clone()));
        }

        if let Some(before_sk) = &request.before_sk {
            condition.push_str(" AND #sk < :before_sk");
            builder = builder
                .expression_attribute_names("#sk", SK)
                .expression_attribute_values(":before_sk", AttributeValue::S(before_sk.clone()));
        }
        let output = builder.key_condition_expression(condition).send().await?;

        let has_more_from_key = output
            .last_evaluated_key
            .as_ref()
            .is_some_and(|key| !key.is_empty());

        let mut records = output
            .items
            .unwrap_or_default()
            .into_iter()
            .map(Record::from_item)
            .collect::<anyhow::Result<Vec<_>>>()?;
        let has_more = has_more_from_key || records.len() > request.limit;

        records.truncate(request.limit);
        Ok(QueryPage { records, has_more })
    }

    async fn commit_workflow(
        &self,
        workflow_instance_id: &str,
        expected_version: u64,
        writes: Vec<DynamoWrite>,
    ) -> Result<(), CommitError> {
        if writes.len() > MAX_TRANSACTION_ITEMS {
            return Err(CommitError::Backend(anyhow::anyhow!(
                "workflow transition needs {} DynamoDB transaction items; maximum is {MAX_TRANSACTION_ITEMS}",
                writes.len()
            )));
        }

        let mut items = Vec::with_capacity(writes.len());

        for write in writes {
            let item = match write {
                DynamoWrite::Put { table, record }
                    if table == Table::WorkflowInstances
                        && record.pk == workflow_instance_id
                        && record.sk == META_SK =>
                {
                    let mut put = Put::builder()
                        .table_name(self.table_name(table))
                        .set_item(Some(record.into_item()));
                    put = if expected_version == 0 {
                        put.condition_expression("attribute_not_exists(#pk)")
                            .expression_attribute_names("#pk", PK)
                    } else {
                        put.condition_expression("#version = :expected")
                            .expression_attribute_names("#version", "version")
                            .expression_attribute_values(
                                ":expected",
                                AttributeValue::S(expected_version.to_string()),
                            )
                    };
                    let put = put
                        .build()
                        .map_err(|error| CommitError::Backend(error.into()))?;
                    TransactWriteItem::builder().put(put).build()
                }
                DynamoWrite::Put { table, record } => TransactWriteItem::builder()
                    .put(
                        Put::builder()
                            .table_name(self.table_name(table))
                            .set_item(Some(record.into_item()))
                            .build()
                            .map_err(|error| CommitError::Backend(error.into()))?,
                    )
                    .build(),
                DynamoWrite::Delete { table, pk, sk } => TransactWriteItem::builder()
                    .delete(
                        Delete::builder()
                            .table_name(self.table_name(table))
                            .set_key(Some(Self::key(&pk, &sk)))
                            .build()
                            .map_err(|error| CommitError::Backend(error.into()))?,
                    )
                    .build(),
            };
            items.push(item);
        }

        if let Err(error) = self
            .client
            .transact_write_items()
            .set_transact_items(Some(items))
            .send()
            .await
        {
            let actual_version = self
                .get(Table::WorkflowInstances, workflow_instance_id, META_SK)
                .await
                .ok()
                .flatten()
                .and_then(|record| record.optional("version")?.parse().ok())
                .unwrap_or(0);
            if actual_version != expected_version {
                return Err(CommitError::Conflict(actual_version));
            }
            return Err(CommitError::Backend(error.into()));
        }
        Ok(())
    }

    async fn update_last_invoked(
        &self,
        workflow_def_id: &str,
        created_at_epoch_ms: u64,
    ) -> anyhow::Result<()> {
        let value = format!("{created_at_epoch_ms:020}");
        let result = self
            .client
            .update_item()
            .table_name(self.table_name(Table::Definitions))
            .set_key(Some(Self::key(
                WORKFLOW_DEFINITION_PK,
                &encode_component(workflow_def_id),
            )))
            .update_expression("SET #last_invoked = :value")
            .condition_expression(
                "attribute_exists(#pk) AND (attribute_not_exists(#last_invoked) OR #last_invoked < :value)",
            )
            .expression_attribute_names("#pk", PK)
            .expression_attribute_names("#last_invoked", "last_invoked_at_epoch_ms")
            .expression_attribute_values(":value", AttributeValue::S(value))
            .send()
            .await;
        match result {
            Ok(_) => Ok(()),
            Err(error) => {
                let existing = self
                    .get(
                        Table::Definitions,
                        WORKFLOW_DEFINITION_PK,
                        &encode_component(workflow_def_id),
                    )
                    .await?;
                if existing
                    .as_ref()
                    .and_then(|record| record.optional("last_invoked_at_epoch_ms"))
                    .is_some_and(|last| last >= format!("{created_at_epoch_ms:020}").as_str())
                {
                    Ok(())
                } else {
                    Err(error.into())
                }
            }
        }
    }
}

struct S3ObjectStore {
    client: aws_sdk_s3::Client,
    bucket: String,
}

#[async_trait]
impl ObjectStore for S3ObjectStore {
    async fn put(&self, key: &str, body: Vec<u8>) -> anyhow::Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(body))
            .send()
            .await?;
        Ok(())
    }

    async fn get(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        match self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(output) => Ok(Some(output.body.collect().await?.into_bytes().to_vec())),
            Err(error)
                if error
                    .as_service_error()
                    .is_some_and(|error| error.is_no_such_key()) =>
            {
                Ok(None)
            }
            Err(error) => Err(error.into()),
        }
    }
}

pub struct AwsStorage {
    dynamo: Arc<dyn DynamoStore>,
    objects: Arc<dyn ObjectStore>,
    prefix: String,
}

impl AwsStorage {
    pub async fn connect(config: AwsStorageConfig) -> anyhow::Result<Self> {
        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
        if let Some(region) = &config.region {
            loader = loader.region(aws_config::Region::new(region.clone()));
        }
        let shared = loader.load().await;
        let mut dynamo_config = aws_sdk_dynamodb::config::Builder::from(&shared);
        let mut s3_config = aws_sdk_s3::config::Builder::from(&shared);
        if let Some(endpoint_url) = &config.endpoint_url {
            dynamo_config = dynamo_config.endpoint_url(endpoint_url);
            s3_config = s3_config.endpoint_url(endpoint_url).force_path_style(true);
        }
        let table_names = HashMap::from([
            (Table::Definitions, config.definitions_table),
            (Table::WorkflowInstances, config.workflow_instances_table),
            (Table::WorkflowEvents, config.workflow_events_table),
            (Table::Tasks, config.tasks_table),
        ]);
        Ok(Self {
            dynamo: Arc::new(AwsDynamoStore {
                client: aws_sdk_dynamodb::Client::from_conf(dynamo_config.build()),
                table_names,
            }),
            objects: Arc::new(S3ObjectStore {
                client: aws_sdk_s3::Client::from_conf(s3_config.build()),
                bucket: config.bucket,
            }),
            prefix: config.prefix,
        })
    }

    #[cfg(test)]
    fn with_stores(
        dynamo: Arc<dyn DynamoStore>,
        objects: Arc<dyn ObjectStore>,
        prefix: &str,
    ) -> Self {
        Self {
            dynamo,
            objects,
            prefix: prefix.to_string(),
        }
    }

    fn object_key(&self, kind: &str, id: &str, suffix: &str) -> String {
        let path = format!("{kind}/{}/{suffix}", encode_component(id));
        if self.prefix.is_empty() {
            path
        } else {
            format!("{}/{path}", self.prefix)
        }
    }

    async fn put_immutable_json<T: Serialize + Sync>(
        &self,
        kind: &str,
        id: &str,
        logical_suffix: &str,
        value: &T,
    ) -> StorageResult<String> {
        let body = serde_json::to_vec(value)?;
        let fingerprint = payload_fingerprint(&body);
        let key = self.object_key(kind, id, &format!("{logical_suffix}/{fingerprint}.json"));
        self.objects
            .put(&key, body)
            .await
            .map_err(StorageError::from)?;
        Ok(key)
    }

    async fn get_json<T: DeserializeOwned>(&self, key: &str) -> StorageResult<Option<T>> {
        self.objects
            .get(key)
            .await
            .map_err(StorageError::from)?
            .map(|body| serde_json::from_slice(&body).map_err(StorageError::from))
            .transpose()
    }
}

fn encode_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

fn payload_fingerprint(payload: &[u8]) -> String {
    Sha256::digest(payload)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn shard_for(id: &str) -> u8 {
    let hash = id.bytes().fold(2_166_136_261_u32, |hash, byte| {
        hash.wrapping_mul(16_777_619) ^ u32::from(byte)
    });
    (hash % u32::from(LIST_SHARD_COUNT)) as u8
}

fn summary_sk(modified_at_epoch_ms: u64, id: &str) -> String {
    format!("{modified_at_epoch_ms:020}#{}", encode_component(id))
}

fn event_sk(sequence: u64) -> String {
    format!("{sequence:020}")
}

fn parse_u64(record: &Record, name: &str) -> anyhow::Result<u64> {
    Ok(record.required(name)?.parse()?)
}

fn parse_optional_u64(record: &Record, name: &str) -> anyhow::Result<Option<u64>> {
    record
        .optional(name)
        .map(str::parse)
        .transpose()
        .map_err(Into::into)
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

fn workflow_info_record(partition: String, info: &WorkflowInfo) -> Record {
    Record::new(partition, summary_sk(info.modified_at_epoch_ms, &info.id))
        .field("id", &info.id)
        .field("workflow_def_id", &info.workflow_def_id)
        .field("status", workflow_status_name(&info.status))
        .optional_field("created_at_epoch_ms", info.created_at_epoch_ms)
        .field("modified_at_epoch_ms", info.modified_at_epoch_ms)
        .optional_field("completed_at_epoch_ms", info.completed_at_epoch_ms)
        .field("total_task_count", info.total_task_count)
        .field("completed_task_count", info.completed_task_count)
}

fn workflow_info_from_record(record: &Record) -> anyhow::Result<WorkflowInfo> {
    Ok(WorkflowInfo {
        id: record.required("id")?.to_string(),
        workflow_def_id: record.required("workflow_def_id")?.to_string(),
        created_at_epoch_ms: parse_optional_u64(record, "created_at_epoch_ms")?,
        modified_at_epoch_ms: parse_u64(record, "modified_at_epoch_ms")?,
        completed_at_epoch_ms: parse_optional_u64(record, "completed_at_epoch_ms")?,
        status: workflow_status_from_name(record.required("status")?)?,
        total_task_count: record.required("total_task_count")?.parse()?,
        completed_task_count: record.required("completed_task_count")?.parse()?,
    })
}

fn list_partitions(info: &WorkflowInfo) -> [String; 4] {
    let shard = shard_for(&info.id);
    let status = workflow_status_name(&info.status);
    let def = encode_component(&info.workflow_def_id);
    [
        format!("LIST#ALL#{shard:02}"),
        format!("LIST#STATUS#{status}#{shard:02}"),
        format!("LIST#DEF#{def}#{shard:02}"),
        format!("LIST#DEF_STATUS#{def}#{status}#{shard:02}"),
    ]
}

fn requested_list_partitions(filters: &[WorkflowInstanceFilter]) -> Vec<String> {
    let definition = filters.iter().find_map(|filter| match filter {
        WorkflowInstanceFilter::WorkflowDefId(id) => Some(encode_component(id)),
        _ => None,
    });
    let statuses = filters.iter().find_map(|filter| match filter {
        WorkflowInstanceFilter::Statuses(statuses) => Some(statuses.as_slice()),
        _ => None,
    });
    let mut partitions = Vec::new();
    for shard in 0..LIST_SHARD_COUNT {
        match (&definition, statuses) {
            (Some(def), Some(statuses)) => partitions.extend(statuses.iter().map(|status| {
                format!(
                    "LIST#DEF_STATUS#{def}#{}#{shard:02}",
                    workflow_status_name(status)
                )
            })),
            (Some(def), None) => partitions.push(format!("LIST#DEF#{def}#{shard:02}")),
            (None, Some(statuses)) => {
                partitions.extend(statuses.iter().map(|status| {
                    format!("LIST#STATUS#{}#{shard:02}", workflow_status_name(status))
                }))
            }
            (None, None) => partitions.push(format!("LIST#ALL#{shard:02}")),
        }
    }
    partitions
}

fn workflow_completed_at(instance: &WorkflowInstance, modified_at_epoch_ms: u64) -> Option<u64> {
    matches!(
        instance.status,
        WorkflowStatus::Completed | WorkflowStatus::Failed
    )
    .then_some(modified_at_epoch_ms)
}

fn task_record(
    workflow_instance_id: &str,
    task_attempt_id: &str,
    task: &TaskInstance,
    workflow_version: u64,
    payload_key: &str,
) -> StorageResult<Record> {
    Ok(
        Record::new(workflow_instance_id, encode_component(task_attempt_id))
            .field("task_attempt_id", task_attempt_id)
            .field("task_def_id", &task.task_def_id)
            .field("status", task_status_name(&task.status))
            .field(
                "satisfaction",
                serde_json::to_string(&task.satisfaction_status)?,
            )
            .field("generation_index", task.generation_index)
            .field("workflow_version", workflow_version)
            .field("payload_key", payload_key),
    )
}

#[async_trait]
impl StoragePort for AwsStorage {
    async fn get_workflow_def(&self, id: &str) -> StorageResult<Option<WorkflowDef>> {
        let Some(record) = self
            .dynamo
            .get(
                Table::Definitions,
                WORKFLOW_DEFINITION_PK,
                &encode_component(id),
            )
            .await
            .map_err(StorageError::from)?
        else {
            return Ok(None);
        };
        self.get_json(record.required("payload_key")?).await
    }

    async fn list_workflow_def(&self) -> StorageResult<Vec<WorkflowDefSummary>> {
        let records = self
            .dynamo
            .query_all(Table::Definitions, WORKFLOW_DEFINITION_PK)
            .await
            .map_err(StorageError::from)?;
        let mut summaries = records
            .into_iter()
            .map(|record| {
                Ok(WorkflowDefSummary {
                    id: record.required("id")?.to_string(),
                    description: record.required("description")?.to_string(),
                    created_at_epoch_ms: parse_u64(&record, "created_at_epoch_ms")?,
                    last_invoked_at_epoch_ms: parse_optional_u64(
                        &record,
                        "last_invoked_at_epoch_ms",
                    )?,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        summaries.sort_by(|left, right| {
            right
                .created_at_epoch_ms
                .cmp(&left.created_at_epoch_ms)
                .then_with(|| right.id.cmp(&left.id))
        });
        Ok(summaries)
    }

    async fn get_function_def(&self, id: &str) -> StorageResult<Option<FunctionDef>> {
        let Some(record) = self
            .dynamo
            .get(
                Table::Definitions,
                FUNCTION_DEFINITION_PK,
                &encode_component(id),
            )
            .await
            .map_err(StorageError::from)?
        else {
            return Ok(None);
        };
        self.get_json(record.required("payload_key")?).await
    }

    async fn get_workflow_instance(&self, id: &str) -> StorageResult<Option<WorkflowInstance>> {
        let Some(record) = self
            .dynamo
            .get(Table::WorkflowInstances, id, META_SK)
            .await
            .map_err(StorageError::from)?
        else {
            return Ok(None);
        };
        self.get_json(record.required("payload_key")?).await
    }

    async fn list_workflow_instance_events(
        &self,
        workflow_instance_id: &str,
        page: WorkflowEventPageRequest,
    ) -> StorageResult<WorkflowEventPage> {
        if page.limit == 0 {
            return Ok(WorkflowEventPage {
                items: vec![],
                next_cursor: None,
            });
        }
        let result = self
            .dynamo
            .query_page(
                Table::WorkflowEvents,
                QueryRequest {
                    pk: workflow_instance_id.to_string(),
                    after_sk: page.cursor.map(event_sk),
                    before_sk: None,
                    descending: false,
                    limit: page.limit,
                },
            )
            .await
            .map_err(StorageError::from)?;
        let next_cursor = result
            .has_more
            .then(|| result.records.last())
            .flatten()
            .map(|record| record.sk.parse::<u64>())
            .transpose()
            .map_err(anyhow::Error::from)?;
        let mut events = Vec::with_capacity(result.records.len());
        for record in result.records {
            events.push(
                self.get_json(record.required("payload_key")?)
                    .await?
                    .ok_or_else(|| {
                        StorageError::from(anyhow::anyhow!(
                            "workflow event payload {} is missing from S3",
                            record.required("payload_key").unwrap_or("<unknown>")
                        ))
                    })?,
            );
        }
        Ok(WorkflowEventPage {
            items: events,
            next_cursor,
        })
    }

    async fn list_workflow_info(
        &self,
        page: WorkflowInfoPageRequest,
        filters: Vec<WorkflowInstanceFilter>,
    ) -> StorageResult<WorkflowInfoPage> {
        if filters.iter().any(
            |filter| matches!(filter, WorkflowInstanceFilter::Statuses(statuses) if statuses.is_empty()),
        ) || page.limit == 0
        {
            return Ok(WorkflowInfoPage {
                items: vec![],
                next_cursor: None,
            });
        }

        let before_sk = page
            .cursor
            .as_ref()
            .map(|cursor| summary_sk(cursor.modified_at_epoch_ms, &cursor.workflow_instance_id));
        let partitions = requested_list_partitions(&filters)
            .into_iter()
            .collect::<HashSet<_>>();
        let mut queries = tokio::task::JoinSet::new();
        for partition in partitions {
            let dynamo = Arc::clone(&self.dynamo);
            let before_sk = before_sk.clone();
            let limit = page.limit + 1;
            queries.spawn(async move {
                dynamo
                    .query_page(
                        Table::WorkflowInstances,
                        QueryRequest {
                            pk: partition,
                            after_sk: None,
                            before_sk,
                            descending: true,
                            limit,
                        },
                    )
                    .await
            });
        }
        let mut records = Vec::new();
        let mut shard_has_more = false;
        while let Some(result) = queries.join_next().await {
            let page = result
                .map_err(anyhow::Error::from)?
                .map_err(StorageError::from)?;
            shard_has_more |= page.has_more;
            records.extend(page.records);
        }

        let mut workflows = records
            .iter()
            .map(workflow_info_from_record)
            .collect::<anyhow::Result<Vec<_>>>()?;
        workflows.sort_by(|left, right| {
            right
                .modified_at_epoch_ms
                .cmp(&left.modified_at_epoch_ms)
                .then_with(|| right.id.cmp(&left.id))
        });
        workflows.dedup_by(|left, right| left.id == right.id);
        let has_more = shard_has_more || workflows.len() > page.limit;
        workflows.truncate(page.limit);
        let next_cursor =
            has_more
                .then(|| workflows.last())
                .flatten()
                .map(|info| WorkflowInfoCursor {
                    modified_at_epoch_ms: info.modified_at_epoch_ms,
                    workflow_instance_id: info.id.clone(),
                });
        Ok(WorkflowInfoPage {
            items: workflows,
            next_cursor,
        })
    }

    async fn save_workflow_def(&self, def: WorkflowDef) -> StorageResult<()> {
        let sk = encode_component(&def.id);
        let existing = self
            .dynamo
            .get(Table::Definitions, WORKFLOW_DEFINITION_PK, &sk)
            .await
            .map_err(StorageError::from)?;

        let created_at_epoch_ms = existing
            .as_ref()
            .map(|record| parse_u64(record, "created_at_epoch_ms"))
            .transpose()?
            .unwrap_or(unix_timestamp_ms()?);

        let payload_key = self
            .put_immutable_json("workflow-definitions", &def.id, "versions", &def)
            .await?;

        let mut record = Record::new(WORKFLOW_DEFINITION_PK, sk)
            .field("id", &def.id)
            .field("description", &def.description)
            .field("created_at_epoch_ms", created_at_epoch_ms)
            .field("payload_key", payload_key);

        if let Some(last_invoked) = existing
            .as_ref()
            .and_then(|record| record.optional("last_invoked_at_epoch_ms"))
        {
            record = record.field("last_invoked_at_epoch_ms", last_invoked);
        }

        self.dynamo
            .put(Table::Definitions, record)
            .await
            .map_err(StorageError::from)
    }

    // Store metadata and function contents in S3
    async fn save_function_def(&self, def: FunctionDef) -> StorageResult<()> {
        let payload_key = self
            .put_immutable_json("function-definitions", &def.id, "versions", &def)
            .await?;

        self.dynamo
            .put(
                Table::Definitions,
                Record::new(FUNCTION_DEFINITION_PK, encode_component(&def.id))
                    .field("id", &def.id)
                    .field("payload_key", payload_key),
            )
            .await
            .map_err(StorageError::from)
    }

    // Only delete metadata
    async fn delete_function_def(&self, id: &str) -> StorageResult<bool> {
        self.dynamo
            .delete(
                Table::Definitions,
                FUNCTION_DEFINITION_PK,
                &encode_component(id),
            )
            .await
            .map_err(StorageError::from)
    }

    async fn save_workflow_instance(
        &self,
        expected_version: u64,
        events: Vec<WorkflowEventRecord>,
        instance: WorkflowInstance,
    ) -> StorageResult<()> {
        let existing = self
            .dynamo
            .get(Table::WorkflowInstances, &instance.id, META_SK)
            .await
            .map_err(StorageError::from)?;

        let actual_version = existing
            .as_ref()
            .map(|record| parse_u64(record, "version"))
            .transpose()?
            .unwrap_or(0);

        // early check to prevent further work if we already see version mismatch
        if actual_version != expected_version {
            return Err(WorkflowVersionConflict {
                workflow_instance_id: instance.id,
                expected_version,
                actual_version,
            }
            .into());
        }

        let first_event_time = events
            .first()
            .map(|event| event.created_time)
            .unwrap_or(unix_timestamp_ms()?);

        let modified_at_epoch_ms = events
            .last()
            .map(|event| event.created_time)
            .unwrap_or(first_event_time);

        let created_at_epoch_ms = existing
            .as_ref()
            .map(|record| parse_u64(record, "created_at_epoch_ms"))
            .transpose()?
            .unwrap_or(first_event_time);

        let completed_at_epoch_ms = existing
            .as_ref()
            .map(|record| parse_optional_u64(record, "completed_at_epoch_ms"))
            .transpose()?
            .flatten()
            .or_else(|| workflow_completed_at(&instance, modified_at_epoch_ms));

        let snapshot_key = self
            .put_immutable_json(
                "workflow-instances",
                &instance.id,
                &format!("versions/{:020}", instance.version),
                &instance,
            )
            .await?;

        let mut writes = Vec::new();
        let task_attempt_ids = if existing.is_none() {
            instance.tasks.keys().cloned().collect::<HashSet<_>>()
        } else {
            changed_task_attempt_ids(&events)
        };
        for task_attempt_id in task_attempt_ids {
            let task = instance.tasks.get(&task_attempt_id).ok_or_else(|| {
                StorageError::Backend(anyhow::anyhow!(
                    "event identified task attempt {task_attempt_id} but it is missing from workflow instance {}",
                    instance.id
                ))
            })?;
            let task_key = self
                .put_immutable_json(
                    "workflow-tasks",
                    &instance.id,
                    &format!(
                        "versions/{:020}/{}",
                        instance.version,
                        encode_component(&task_attempt_id)
                    ),
                    task,
                )
                .await?;
            writes.push(DynamoWrite::Put {
                table: Table::Tasks,
                record: task_record(
                    &instance.id,
                    &task_attempt_id,
                    task,
                    instance.version,
                    &task_key,
                )?,
            });
        }

        let info = WorkflowInfo::from_instance_with_timestamps(
            &instance,
            Some(created_at_epoch_ms),
            modified_at_epoch_ms,
            completed_at_epoch_ms,
        );
        let new_projection_keys = list_partitions(&info)
            .into_iter()
            .map(|pk| (pk, summary_sk(info.modified_at_epoch_ms, &info.id)))
            .collect::<HashSet<_>>();
        if let Some(existing) = &existing {
            let old_info = workflow_info_from_record(existing)?;
            for pk in list_partitions(&old_info) {
                let sk = summary_sk(old_info.modified_at_epoch_ms, &old_info.id);
                if !new_projection_keys.contains(&(pk.clone(), sk.clone())) {
                    writes.push(DynamoWrite::Delete {
                        table: Table::WorkflowInstances,
                        pk,
                        sk,
                    });
                }
            }
        }

        let mut metadata = workflow_info_record(instance.id.clone(), &info)
            .field("version", instance.version)
            .field("payload_key", &snapshot_key);
        metadata.sk = META_SK.to_string();
        writes.push(DynamoWrite::Put {
            table: Table::WorkflowInstances,
            record: metadata,
        });
        writes.extend(
            list_partitions(&info)
                .into_iter()
                .map(|partition| DynamoWrite::Put {
                    table: Table::WorkflowInstances,
                    record: workflow_info_record(partition, &info),
                }),
        );

        for (index, event) in events.iter().enumerate() {
            let sequence = expected_version + index as u64 + 1;
            let event_key = self
                .put_immutable_json(
                    "workflow-events",
                    &instance.id,
                    &format!("events/{sequence:020}"),
                    event,
                )
                .await?;
            writes.push(DynamoWrite::Put {
                table: Table::WorkflowEvents,
                record: Record::new(&instance.id, event_sk(sequence))
                    .field("created_at_epoch_ms", event.created_time)
                    .field("workflow_version", instance.version)
                    .field("payload_key", event_key),
            });
        }

        match self
            .dynamo
            .commit_workflow(&instance.id, expected_version, writes)
            .await
        {
            Ok(()) => {}
            Err(CommitError::Conflict(actual_version)) => {
                return Err(WorkflowVersionConflict {
                    workflow_instance_id: instance.id,
                    expected_version,
                    actual_version,
                }
                .into());
            }
            Err(CommitError::Backend(error)) => return Err(StorageError::from(error)),
        }

        if expected_version == 0
            && let Err(error) = self
                .dynamo
                .update_last_invoked(&instance.workflow_def_id, created_at_epoch_ms)
                .await
        {
            warn!(
                workflow_def_id = %instance.workflow_def_id,
                workflow_instance_id = %instance.id,
                %error,
                "Failed to update workflow definition last-invoked projection"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use serde_json::json;

    use crate::core::function::models::FunctionDependency;
    use crate::core::task::{TaskInputMapping, TaskSatisfactionStatus};
    use crate::core::verifier::{VerifierAttemptMetadata, VerifierAttemptStatus, VerifierDecision};
    use crate::core::worker::WorkerHostId;
    use crate::core::workflow::events::{WorkflowEventRecord, WorkflowInstanceEvent};
    use crate::core::workflow::models::{
        VerifierFeedbackEntry, VerifierGenerationState, VerifierStateStatus,
    };
    use crate::ports::storage::{StorageError, WorkflowInfoPageRequest};

    #[derive(Default)]
    struct FakeDynamoStore {
        records: Mutex<HashMap<(Table, String, String), Record>>,
        query_limits: Mutex<Vec<usize>>,
        last_commit_write_count: Mutex<usize>,
        fail_next_commit: Mutex<bool>,
    }

    impl FakeDynamoStore {
        fn table_records(&self, table: Table) -> Vec<Record> {
            self.records
                .lock()
                .unwrap()
                .iter()
                .filter(|((stored_table, _, _), _)| *stored_table == table)
                .map(|(_, record)| record.clone())
                .collect()
        }
    }

    #[async_trait]
    impl DynamoStore for FakeDynamoStore {
        async fn get(&self, table: Table, pk: &str, sk: &str) -> anyhow::Result<Option<Record>> {
            Ok(self
                .records
                .lock()
                .unwrap()
                .get(&(table, pk.to_string(), sk.to_string()))
                .cloned())
        }

        async fn put(&self, table: Table, record: Record) -> anyhow::Result<()> {
            self.records
                .lock()
                .unwrap()
                .insert((table, record.pk.clone(), record.sk.clone()), record);
            Ok(())
        }

        async fn delete(&self, table: Table, pk: &str, sk: &str) -> anyhow::Result<bool> {
            Ok(self
                .records
                .lock()
                .unwrap()
                .remove(&(table, pk.to_string(), sk.to_string()))
                .is_some())
        }

        async fn query_page(
            &self,
            table: Table,
            request: QueryRequest,
        ) -> anyhow::Result<QueryPage> {
            self.query_limits.lock().unwrap().push(request.limit);
            let mut records = self
                .records
                .lock()
                .unwrap()
                .iter()
                .filter(|((stored_table, pk, sk), _)| {
                    *stored_table == table
                        && pk == &request.pk
                        && request.after_sk.as_ref().is_none_or(|after| sk > after)
                        && request.before_sk.as_ref().is_none_or(|before| sk < before)
                })
                .map(|(_, record)| record.clone())
                .collect::<Vec<_>>();
            records.sort_by(|left, right| left.sk.cmp(&right.sk));
            if request.descending {
                records.reverse();
            }
            let has_more = records.len() > request.limit;
            records.truncate(request.limit);
            Ok(QueryPage { records, has_more })
        }

        async fn commit_workflow(
            &self,
            workflow_instance_id: &str,
            expected_version: u64,
            writes: Vec<DynamoWrite>,
        ) -> Result<(), CommitError> {
            *self.last_commit_write_count.lock().unwrap() = writes.len();
            if writes.len() > MAX_TRANSACTION_ITEMS {
                return Err(CommitError::Backend(anyhow::anyhow!(
                    "workflow transition needs {} DynamoDB transaction items; maximum is {MAX_TRANSACTION_ITEMS}",
                    writes.len()
                )));
            }
            if std::mem::take(&mut *self.fail_next_commit.lock().unwrap()) {
                return Err(CommitError::Backend(anyhow::anyhow!(
                    "injected transaction failure"
                )));
            }
            let mut records = self.records.lock().unwrap();
            let actual_version = records
                .get(&(
                    Table::WorkflowInstances,
                    workflow_instance_id.to_string(),
                    META_SK.to_string(),
                ))
                .and_then(|record| record.optional("version"))
                .and_then(|version| version.parse().ok())
                .unwrap_or(0);
            if actual_version != expected_version {
                return Err(CommitError::Conflict(actual_version));
            }
            let mut updated = records.clone();
            for write in writes {
                match write {
                    DynamoWrite::Put { table, record } => {
                        updated.insert((table, record.pk.clone(), record.sk.clone()), record);
                    }
                    DynamoWrite::Delete { table, pk, sk } => {
                        updated.remove(&(table, pk, sk));
                    }
                }
            }
            *records = updated;
            Ok(())
        }

        async fn update_last_invoked(
            &self,
            workflow_def_id: &str,
            created_at_epoch_ms: u64,
        ) -> anyhow::Result<()> {
            if let Some(record) = self.records.lock().unwrap().get_mut(&(
                Table::Definitions,
                WORKFLOW_DEFINITION_PK.to_string(),
                encode_component(workflow_def_id),
            )) {
                let replace = record
                    .optional("last_invoked_at_epoch_ms")
                    .and_then(|value| value.parse::<u64>().ok())
                    .is_none_or(|current| current < created_at_epoch_ms);
                if replace {
                    record.fields.insert(
                        "last_invoked_at_epoch_ms".to_string(),
                        created_at_epoch_ms.to_string(),
                    );
                }
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeObjectStore {
        objects: Mutex<HashMap<String, Vec<u8>>>,
    }

    #[async_trait]
    impl ObjectStore for FakeObjectStore {
        async fn put(&self, key: &str, body: Vec<u8>) -> anyhow::Result<()> {
            self.objects.lock().unwrap().insert(key.to_string(), body);
            Ok(())
        }

        async fn get(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
            Ok(self.objects.lock().unwrap().get(key).cloned())
        }
    }

    fn storage() -> (AwsStorage, Arc<FakeDynamoStore>, Arc<FakeObjectStore>) {
        let dynamo = Arc::new(FakeDynamoStore::default());
        let objects = Arc::new(FakeObjectStore::default());
        (
            AwsStorage::with_stores(dynamo.clone(), objects.clone(), "test"),
            dynamo,
            objects,
        )
    }

    fn workflow_def(id: &str) -> WorkflowDef {
        WorkflowDef {
            id: id.to_string(),
            description: format!("{id} description"),
            tasks: vec![],
            data_bindings: vec![],
        }
    }

    fn event(created_time: u64, status: WorkflowStatus) -> WorkflowEventRecord {
        WorkflowEventRecord {
            created_time,
            event: WorkflowInstanceEvent::WorkflowStatusChanged { status },
        }
    }

    fn instance(
        id: &str,
        workflow_def_id: &str,
        version: u64,
        status: WorkflowStatus,
    ) -> WorkflowInstance {
        WorkflowInstance {
            id: id.to_string(),
            workflow_def_id: workflow_def_id.to_string(),
            version,
            status,
            trigger_input: Some(json!({"source": "test"})),
            pinned_worker_host: Some(WorkerHostId("host-a".to_string())),
            tasks: HashMap::new(),
            verifier_states: HashMap::new(),
        }
    }

    fn task(task_def_id: &str, status: TaskStatus) -> TaskInstance {
        TaskInstance {
            task_def_id: task_def_id.to_string(),
            status,
            satisfaction_status: TaskSatisfactionStatus::Pending,
            human_input: Some(json!("yes")),
            input_data: vec![json!({"input": 1})],
            input_mapping: vec![TaskInputMapping {
                task_id: "source".to_string(),
                generation: 1,
            }],
            output_data: Some(json!({"ok": true})),
            generation_index: 1,
            verifier_metadata: Some(VerifierAttemptMetadata {
                status: VerifierAttemptStatus::Accepted,
                decision: Some(VerifierDecision::Complete),
                feedback: Some("done".to_string()),
                verifier_output: Some(json!({"decision": "complete"})),
                exit_reason: None,
            }),
        }
    }

    async fn save_transition(
        storage: &AwsStorage,
        current: Option<&WorkflowInstance>,
        updated: WorkflowInstance,
        events: Vec<WorkflowEventRecord>,
    ) -> StorageResult<()> {
        storage
            .save_workflow_instance(
                current.map(|instance| instance.version).unwrap_or(0),
                events,
                updated,
            )
            .await
    }

    fn page_request(limit: usize, cursor: Option<WorkflowInfoCursor>) -> WorkflowInfoPageRequest {
        WorkflowInfoPageRequest { limit, cursor }
    }

    fn assert_content_addressed_key(key: &str, body: &[u8]) {
        assert!(
            key.ends_with(&format!("/{}.json", payload_fingerprint(body))),
            "object key does not end with its content fingerprint: {key}"
        );
    }

    #[tokio::test]
    async fn stores_definitions_and_functions_in_definitions_table_with_s3_payloads() {
        let (storage, dynamo, objects) = storage();
        storage
            .save_workflow_def(workflow_def("deploy"))
            .await
            .unwrap();
        let function = FunctionDef {
            id: "large-function".to_string(),
            dependencies: vec![FunctionDependency {
                name: "library".to_string(),
                version: "1.0.0".to_string(),
            }],
            code: "x".repeat(500_000),
        };
        storage.save_function_def(function).await.unwrap();

        assert_eq!(
            storage
                .get_workflow_def("deploy")
                .await
                .unwrap()
                .unwrap()
                .id,
            "deploy"
        );
        assert_eq!(
            storage
                .get_function_def("large-function")
                .await
                .unwrap()
                .unwrap()
                .code
                .len(),
            500_000
        );
        assert_eq!(
            dynamo.table_records(Table::Definitions).len(),
            2,
            "both definition types share only the definitions table"
        );
        let stored_objects = objects.objects.lock().unwrap();
        assert!(stored_objects.values().any(|body| body.len() > 400_000));
        assert!(
            stored_objects
                .keys()
                .any(|key| { key.starts_with("test/workflow-definitions/deploy/versions/") })
        );
        assert!(
            stored_objects.keys().any(|key| {
                key.starts_with("test/function-definitions/large-function/versions/")
            })
        );
        for (key, body) in stored_objects.iter() {
            assert_content_addressed_key(key, body);
        }
        drop(stored_objects);
        assert!(storage.delete_function_def("large-function").await.unwrap());
        assert!(!storage.delete_function_def("large-function").await.unwrap());
    }

    #[tokio::test]
    async fn commits_across_tables_using_task_ids_from_events() {
        let (storage, dynamo, objects) = storage();
        storage
            .save_workflow_def(workflow_def("deploy"))
            .await
            .unwrap();
        let mut initial = instance("wf-1", "deploy", 1, WorkflowStatus::Running);
        initial.tasks.insert(
            "unchanged[1]".to_string(),
            task("unchanged", TaskStatus::Completed),
        );
        initial.verifier_states.insert(
            "verify".to_string(),
            VerifierGenerationState {
                verifier_task_id: "verify".to_string(),
                rerun_start_task_id: "added".to_string(),
                latest_generation: 1,
                selected_generation: Some(1),
                feedback_history: vec![VerifierFeedbackEntry {
                    generation_index: 1,
                    feedback: "ok".to_string(),
                    verifier_output: json!({"decision": "complete"}),
                }],
                status: VerifierStateStatus::Accepted,
                exit_reason: None,
            },
        );
        save_transition(
            &storage,
            None,
            initial.clone(),
            vec![WorkflowEventRecord {
                created_time: 100,
                event: WorkflowInstanceEvent::WorkflowCreated {
                    instance: initial.clone(),
                },
            }],
        )
        .await
        .unwrap();

        let mut updated = initial.clone();
        updated.version = 2;
        updated
            .tasks
            .insert("added[1]".to_string(), task("added", TaskStatus::Completed));
        let added_task = updated.tasks["added[1]"].clone();
        save_transition(
            &storage,
            Some(&initial),
            updated.clone(),
            vec![WorkflowEventRecord {
                created_time: 200,
                event: WorkflowInstanceEvent::TaskMaterialized {
                    task_attempt_id: "added[1]".to_string(),
                    task: added_task,
                },
            }],
        )
        .await
        .unwrap();

        let task_records = dynamo.table_records(Table::Tasks);
        assert_eq!(task_records.len(), 2);
        assert!(task_records.iter().any(|record| {
            record.optional("task_attempt_id") == Some("unchanged[1]")
                && record.optional("workflow_version") == Some("1")
        }));
        assert!(task_records.iter().any(|record| {
            record.optional("task_attempt_id") == Some("added[1]")
                && record.optional("workflow_version") == Some("2")
        }));
        assert_eq!(dynamo.table_records(Table::WorkflowEvents).len(), 2);
        assert!(dynamo.table_records(Table::WorkflowInstances).len() >= 5);

        let loaded = storage
            .get_workflow_instance("wf-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.tasks.len(), 2);
        assert_eq!(loaded.trigger_input, Some(json!({"source": "test"})));
        assert_eq!(
            loaded.pinned_worker_host,
            Some(WorkerHostId("host-a".to_string()))
        );
        assert_eq!(
            loaded.verifier_states["verify"].status,
            VerifierStateStatus::Accepted
        );

        let task_payloads = objects
            .objects
            .lock()
            .unwrap()
            .keys()
            .filter(|key| key.contains("workflow-tasks"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            task_payloads.len(),
            2,
            "the unchanged task was not rewritten"
        );
        let stored_objects = objects.objects.lock().unwrap();
        assert!(stored_objects.keys().any(|key| {
            key.starts_with("test/workflow-instances/wf-1/versions/00000000000000000001/")
        }));
        assert!(stored_objects.keys().any(|key| {
            key.starts_with("test/workflow-instances/wf-1/versions/00000000000000000002/")
        }));
        assert!(stored_objects.keys().any(|key| {
            key.starts_with("test/workflow-tasks/wf-1/versions/00000000000000000002/added%5B1%5D/")
        }));
        assert!(stored_objects.keys().any(|key| {
            key.starts_with("test/workflow-events/wf-1/events/00000000000000000002/")
        }));
        for (key, body) in stored_objects.iter() {
            assert_content_addressed_key(key, body);
        }
        drop(stored_objects);
        assert_eq!(
            storage.list_workflow_def().await.unwrap()[0].last_invoked_at_epoch_ms,
            Some(100)
        );
    }

    #[tokio::test]
    async fn event_pages_are_ordered_and_bounded() {
        let (storage, dynamo, _) = storage();
        let workflow = instance("wf-1", "deploy", 3, WorkflowStatus::Running);
        save_transition(
            &storage,
            None,
            workflow,
            vec![
                event(100, WorkflowStatus::Pending),
                event(200, WorkflowStatus::Running),
                event(300, WorkflowStatus::Running),
            ],
        )
        .await
        .unwrap();

        let first = storage
            .list_workflow_instance_events(
                "wf-1",
                WorkflowEventPageRequest {
                    limit: 2,
                    cursor: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            first
                .items
                .iter()
                .map(|event| event.created_time)
                .collect::<Vec<_>>(),
            vec![100, 200]
        );
        assert_eq!(first.next_cursor, Some(2));
        let second = storage
            .list_workflow_instance_events(
                "wf-1",
                WorkflowEventPageRequest {
                    limit: 2,
                    cursor: first.next_cursor,
                },
            )
            .await
            .unwrap();
        assert_eq!(second.items[0].created_time, 300);
        assert_eq!(second.next_cursor, None);
        assert!(
            dynamo
                .query_limits
                .lock()
                .unwrap()
                .iter()
                .all(|limit| *limit <= 2)
        );
    }

    #[tokio::test]
    async fn workflow_listing_reads_bounded_pages_from_stable_shards() {
        let (storage, dynamo, _) = storage();
        {
            let mut records = dynamo.records.lock().unwrap();
            for index in 0..1000_u64 {
                let info = WorkflowInfo {
                    id: format!("wf-{index:04}"),
                    workflow_def_id: "deploy".to_string(),
                    created_at_epoch_ms: Some(index),
                    modified_at_epoch_ms: index,
                    completed_at_epoch_ms: None,
                    status: WorkflowStatus::Running,
                    total_task_count: 1,
                    completed_task_count: 0,
                };
                let partition = list_partitions(&info)[0].clone();
                let record = workflow_info_record(partition, &info);
                records.insert(
                    (
                        Table::WorkflowInstances,
                        record.pk.clone(),
                        record.sk.clone(),
                    ),
                    record,
                );
            }
        }

        let first = storage
            .list_workflow_info(page_request(5, None), vec![])
            .await
            .unwrap();
        assert_eq!(first.items.len(), 5);
        assert_eq!(first.items[0].id, "wf-0999");
        assert!(first.next_cursor.is_some());
        let limits = dynamo.query_limits.lock().unwrap().clone();
        assert_eq!(limits.len(), usize::from(LIST_SHARD_COUNT));
        assert!(limits.iter().all(|limit| *limit == 6));

        dynamo.query_limits.lock().unwrap().clear();
        let second = storage
            .list_workflow_info(page_request(5, first.next_cursor), vec![])
            .await
            .unwrap();
        assert_eq!(second.items.len(), 5);
        assert!(second.items[0].modified_at_epoch_ms < 995);
        assert!(
            dynamo
                .query_limits
                .lock()
                .unwrap()
                .iter()
                .all(|limit| *limit == 6)
        );
    }

    #[tokio::test]
    async fn workflow_listing_uses_exact_combined_filter_projections() {
        let (storage, _, _) = storage();
        for (id, def, time, status) in [
            ("deploy-pending", "deploy", 300, WorkflowStatus::Pending),
            ("deploy-running", "deploy", 200, WorkflowStatus::Running),
            ("other-pending", "other", 100, WorkflowStatus::Pending),
        ] {
            save_transition(
                &storage,
                None,
                instance(id, def, 1, status.clone()),
                vec![event(time, status)],
            )
            .await
            .unwrap();
        }

        let page = storage
            .list_workflow_info(
                page_request(10, None),
                vec![
                    WorkflowInstanceFilter::WorkflowDefId("deploy".to_string()),
                    WorkflowInstanceFilter::Statuses(vec![WorkflowStatus::Pending]),
                ],
            )
            .await
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, "deploy-pending");
    }

    #[tokio::test]
    async fn stale_and_failed_transactions_do_not_change_visible_state() {
        let (storage, dynamo, _) = storage();
        let initial = instance("wf-1", "deploy", 1, WorkflowStatus::Running);
        save_transition(
            &storage,
            None,
            initial.clone(),
            vec![event(100, WorkflowStatus::Running)],
        )
        .await
        .unwrap();

        let stale = instance("wf-1", "deploy", 1, WorkflowStatus::Completed);
        let error = save_transition(
            &storage,
            None,
            stale,
            vec![event(200, WorkflowStatus::Completed)],
        )
        .await
        .unwrap_err();
        assert!(matches!(error, StorageError::WorkflowVersionConflict(_)));

        *dynamo.fail_next_commit.lock().unwrap() = true;
        let mut update = initial.clone();
        update.version = 2;
        update.status = WorkflowStatus::Completed;
        assert!(
            save_transition(
                &storage,
                Some(&initial),
                update,
                vec![event(300, WorkflowStatus::Completed)],
            )
            .await
            .is_err()
        );
        assert_eq!(
            storage
                .get_workflow_instance("wf-1")
                .await
                .unwrap()
                .unwrap()
                .status,
            WorkflowStatus::Running
        );
        assert_eq!(dynamo.table_records(Table::WorkflowEvents).len(), 1);
    }

    #[tokio::test]
    async fn rejects_transitions_over_dynamodb_transaction_limit() {
        let (storage, dynamo, _) = storage();
        let mut workflow = instance("large", "deploy", 1, WorkflowStatus::Running);
        for index in 0..MAX_TRANSACTION_ITEMS {
            workflow.tasks.insert(
                format!("task-{index}[1]"),
                task(&format!("task-{index}"), TaskStatus::Pending),
            );
        }
        let error = save_transition(
            &storage,
            None,
            workflow,
            vec![event(100, WorkflowStatus::Running)],
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("maximum is 100"));
        assert!(
            dynamo
                .get(Table::WorkflowInstances, "large", META_SK)
                .await
                .unwrap()
                .is_none()
        );
    }
}
