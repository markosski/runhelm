---
title: Orchestrator Storage
description: Configure in-memory, SQL, or AWS storage for workflow definitions and workflow state.
---

RunHelm stores workflow definitions, function definitions, workflow instances, workflow events, and workflow list summaries through the orchestrator storage adapter.

By default, the orchestrator uses in-memory storage. This is useful for local development, but workflow state is lost when the orchestrator process exits.

## In-memory storage

In-memory storage is the default:

```bash
RUNHELM_STORAGE=memory
```

You can also omit `RUNHELM_STORAGE`; `memory` is used automatically.

## Durable SQL storage

Use SQL storage when workflow definitions and workflow state should survive orchestrator restarts:

```bash
RUNHELM_STORAGE=sql
RUNHELM_DATABASE_URL=sqlite:///var/lib/runhelm/runhelm.db
```

The SQL adapter initializes its schema automatically on startup and records applied schema migrations in the database.

SQLite is the first supported SQL backend. The storage adapter detects the SQL dialect from `RUNHELM_DATABASE_URL`; Postgres and MySQL URL schemes are reserved for future backend support.

## Persistence model

SQL storage keeps workflow-level state, task attempts, verifier state, and events in separate tables. RunHelm still exposes the same workflow state model through the API.

Workflow definitions may enter and leave the API as JSON or YAML. Storage
normalizes definition payloads to JSON, so the selected API representation does
not change the persisted format.

Workflow transition commits are atomic: when the orchestrator records a workflow change, the SQL adapter saves the event records, workflow row, task rows, and verifier rows together. Workflow list summaries are derived from workflow and task rows when queried.

SQL storage does not make task execution exactly once. Tasks should still be designed for at-least-once execution. See [Reliability and Side Effects](/docs/operations/reliability/).

## Durable AWS storage

Use the AWS adapter for a production-capable backend built on DynamoDB and S3:

```bash
RUNHELM_STORAGE=aws
RUNHELM_AWS_DEFINITIONS_TABLE=runhelm-definitions
RUNHELM_AWS_WORKFLOW_INSTANCES_TABLE=runhelm-workflow-instances
RUNHELM_AWS_WORKFLOW_EVENTS_TABLE=runhelm-workflow-events
RUNHELM_AWS_TASKS_TABLE=runhelm-tasks
RUNHELM_AWS_S3_BUCKET=my-runhelm-state
RUNHELM_AWS_REGION=us-east-1
```

RunHelm uses the standard AWS credential provider chain. In AWS, prefer a task,
pod, or instance role instead of configuring long-lived access keys.

The adapter accepts these settings:

| Setting | Required | Purpose |
| --- | --- | --- |
| `RUNHELM_AWS_DEFINITIONS_TABLE` | Yes | Workflow and function definition metadata. |
| `RUNHELM_AWS_WORKFLOW_INSTANCES_TABLE` | Yes | Current workflow metadata and workflow-list projections. |
| `RUNHELM_AWS_WORKFLOW_EVENTS_TABLE` | Yes | Ordered workflow event indexes. |
| `RUNHELM_AWS_TASKS_TABLE` | Yes | Current queryable task-attempt metadata. |
| `RUNHELM_AWS_S3_BUCKET` | Yes | S3 bucket used for JSON payloads. |
| `RUNHELM_AWS_REGION` | No | AWS region. `AWS_REGION` is used when this setting is omitted. |
| `RUNHELM_AWS_S3_PREFIX` | No | Object key prefix. Defaults to `runhelm`. |
| `RUNHELM_AWS_ENDPOINT_URL` | No | Shared DynamoDB and S3 endpoint for LocalStack or another AWS-compatible development service. |

### DynamoDB tables

Create all four tables with string partition and sort keys named `pk` and `sk`.
No secondary indexes are required by the current adapter. On-demand capacity is
a practical starting point because workflow traffic is often uneven. From the
repository root, run the included idempotent setup script:

```bash
./orchestrator/scripts/setup_aws_dynamodb.sh
```

The script uses the configured `RUNHELM_AWS_*_TABLE` names and otherwise
defaults to the names shown below. It also honors `RUNHELM_AWS_REGION` and
`RUNHELM_AWS_ENDPOINT_URL`. The equivalent manual commands are:

```bash
create_runhelm_table() {
  aws dynamodb create-table \
    --table-name "$1" \
    --attribute-definitions AttributeName=pk,AttributeType=S AttributeName=sk,AttributeType=S \
    --key-schema AttributeName=pk,KeyType=HASH AttributeName=sk,KeyType=RANGE \
    --billing-mode PAY_PER_REQUEST
}

create_runhelm_table runhelm-definitions
create_runhelm_table runhelm-workflow-instances
create_runhelm_table runhelm-workflow-events
create_runhelm_table runhelm-tasks
```

RunHelm does not create the tables or bucket at startup.

The tables have separate access patterns:

- Definitions use definition type as `pk` and definition ID as `sk`.
- Workflow instances use instance ID with a `META` record. Compact list
  projections are distributed over 16 stable shards for all-workflow, status,
  definition, and combined definition/status queries.
- Events use workflow instance ID as `pk` and a zero-padded event sequence as
  `sk`, allowing bounded ascending cursor queries.
- Tasks use workflow instance ID as `pk` and task attempt ID as `sk`.

### Persistence model

Full workflow definitions, function definitions, workflow snapshots, task
payloads, and event payloads are always stored in S3. DynamoDB stores their S3
keys and the fields needed to query workflow summaries, current task state, and
event order. Payload placement does not change based on JSON size.

Definitions, workflow snapshots, task payloads, and event payloads use immutable
keys containing a SHA-256 content fingerprint. A transition writes those S3
objects first, then atomically updates the
snapshot pointer, changed task records, ordered event indexes, and workflow-list
projections with one cross-table DynamoDB transaction. The transaction
condition implements optimistic locking; a stale workflow version is rejected
without making its uploaded payloads visible as current state.

Workflow events identify the task attempts they change. Durable adapters use
those IDs to write authoritative task records from the resulting workflow
snapshot, creating missing records and updating existing records. Transition
saves do not bulk-replace or delete task records, and unchanged tasks are not
rewritten during normal transitions.
Workflow and event list requests use DynamoDB limits and cursor key conditions;
they do not load a complete partition before applying API pagination.

DynamoDB allows at most 100 actions in one transaction. RunHelm rejects a
transition that exceeds this limit instead of splitting an atomic workflow
change into partially visible commits. Failed transitions can leave unreachable
immutable S3 objects; lifecycle rules may remove such objects according to the
deployment's retention policy.

### IAM capabilities

The orchestrator role needs access to the configured resources. At a high
level, allow these actions while scoping resources to the four tables, bucket,
and configured prefix:

- DynamoDB: `GetItem`, `PutItem`, `UpdateItem`, `DeleteItem`, `Query`, and `TransactWriteItems`.
- S3: `GetObject` and `PutObject`.

Encryption, retention, backup, and lifecycle rules remain deployment concerns.
In particular, configure DynamoDB point-in-time recovery separately for each
table and S3 versioning or retention according to your recovery requirements.
RunHelm does not enable event TTL; event-retention policy remains explicit.

### LocalStack development

LocalStack can exercise the backend without real AWS credentials. After
starting LocalStack with DynamoDB and S3 enabled, create the resources against
its endpoint:

```bash
export AWS_ACCESS_KEY_ID=test
export AWS_SECRET_ACCESS_KEY=test
export AWS_REGION=us-east-1

create_local_table() {
  aws --endpoint-url http://localhost:4566 dynamodb create-table \
    --table-name "$1" \
    --attribute-definitions AttributeName=pk,AttributeType=S AttributeName=sk,AttributeType=S \
    --key-schema AttributeName=pk,KeyType=HASH AttributeName=sk,KeyType=RANGE \
    --billing-mode PAY_PER_REQUEST
}

create_local_table runhelm-definitions
create_local_table runhelm-workflow-instances
create_local_table runhelm-workflow-events
create_local_table runhelm-tasks
aws --endpoint-url http://localhost:4566 s3 mb s3://runhelm-state
```

Then start the orchestrator with:

```bash
RUNHELM_STORAGE=aws \
RUNHELM_AWS_DEFINITIONS_TABLE=runhelm-definitions \
RUNHELM_AWS_WORKFLOW_INSTANCES_TABLE=runhelm-workflow-instances \
RUNHELM_AWS_WORKFLOW_EVENTS_TABLE=runhelm-workflow-events \
RUNHELM_AWS_TASKS_TABLE=runhelm-tasks \
RUNHELM_AWS_S3_BUCKET=runhelm-state \
RUNHELM_AWS_ENDPOINT_URL=http://localhost:4566 \
cargo run
```

The adapter's automated tests use in-memory DynamoDB and object-store fakes, so
the AWS storage test suite itself does not require credentials or LocalStack.
