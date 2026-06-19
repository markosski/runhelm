# Cloud Storage Backend Evaluation for StoragePort

This document evaluates DynamoDB and S3 as cloud backends for the orchestrator `StoragePort`.

## Current StoragePort Shape

`StoragePort` persists three resource types:

- workflow definitions
- reusable function definitions
- workflow instances

The current contract supports direct reads by id, whole-object saves, function deletion, listing all workflow instances, and listing active workflow instances.

`list_active_workflow_instances` is the most important backend driver. The in-memory adapter derives activity by checking:

- workflow status is `Pending` or `Running`
- any task status is `Pending` or `Running`
- any verifier state is `Running`

A cloud implementation should avoid scanning every workflow instance to answer this during startup recovery.

## DynamoDB

DynamoDB is the better primary backend for the current contract.

Recommended table shape:

- table: `runhelm_orchestrator_state`
- partition key: `pk`
- sort key: `sk`
- item types:
  - `pk = WORKFLOW_DEF#{id}`, `sk = METADATA`
  - `pk = FUNCTION_DEF#{id}`, `sk = METADATA`
  - `pk = WORKFLOW_INSTANCE#{id}`, `sk = METADATA`
- attributes:
  - `resource_type`
  - `id`
  - `status`
  - `active`
  - `updated_at`
  - `version`
  - `document`

Recommended indexes:

- GSI `resource_type_updated_at`: `resource_type` partition key, `updated_at` sort key, for listing workflow instances.
- GSI `active_updated_at`: `active` partition key, `updated_at` sort key, for startup recovery and active workflow requeue.

Strengths:

- Fast point reads by id.
- Native query support for active instances through a GSI.
- Conditional writes can support optimistic concurrency through a `version` field.
- TTL can be added later for completed instance retention policies.
- DynamoDB Streams can later feed observability, audit, or event-sourcing pipelines.

Weaknesses:

- DynamoDB has a 400 KB item size limit, so large task outputs, verifier feedback history, transcripts, or function bodies can exceed a single-item design.
- Whole-instance overwrite saves become increasingly expensive as workflow instances grow.
- If multiple orchestrator processes can mutate the same workflow instance concurrently, the current port lacks an explicit compare-and-swap API. A DynamoDB implementation can still protect writes internally, but the service layer will need retry behavior to resolve version conflicts.

Best fit:

- Primary workflow metadata and state.
- Definitions, status fields, active indexes, and compact workflow-instance documents.

## S3

S3 is a weak primary backend for `StoragePort`, but a strong companion backend for large immutable or infrequently read blobs.

Possible key layout:

- `workflow-defs/{id}.json`
- `function-defs/{id}.json`
- `workflow-instances/{id}.json`
- optional active marker objects: `active-workflow-instances/{id}.json`

Strengths:

- Very high object size ceiling relative to workflow documents.
- Simple JSON object storage maps naturally to whole-object save/read.
- Strong read-after-write and list consistency removes older S3 listing caveats.
- Versioning can preserve workflow state history with low implementation effort.
- Cheap durable storage for large task outputs, logs, transcripts, workspace manifests, and audit snapshots.

Weaknesses:

- No native secondary indexes. Active workflow listing requires either scanning all instance objects or maintaining separate active marker objects.
- Conditional writes exist, but optimistic concurrency is awkward compared with DynamoDB and depends on ETags.
- Listing all workflow instances becomes paginated object-prefix traversal rather than a query over indexed state.
- S3 request latency is usually a worse fit for hot state-machine progress updates.

Best fit:

- Large task outputs.
- Execution artifacts.
- Immutable workflow snapshots.
- Overflow documents referenced from DynamoDB when a workflow instance exceeds DynamoDB size limits.

## Recommended Direction

Use DynamoDB as the primary `StoragePort` backend, with an explicit derived `active` attribute maintained on every `save_workflow_instance`.

Use S3 as an optional blob backend behind the DynamoDB adapter:

- Store compact workflow instances directly in DynamoDB.
- Store large JSON payloads or selected large fields in S3.
- Keep DynamoDB as the source of queryable metadata and indexes.
- Store S3 object keys in the DynamoDB `document` or companion attributes.

This preserves the current `StoragePort` contract while giving startup recovery an indexed query path.

## Suggested Implementation Phases

1. Add serde helpers/tests for stable JSON round trips of `WorkflowDef`, `FunctionDef`, and `WorkflowInstance`.
2. Add a `DynamoDbStorage` adapter behind a feature flag or runtime config.
3. Maintain `active` and `updated_at` attributes on workflow instance saves.
4. Add optimistic-write support with an internal `version` field.
5. Add optional S3 overflow for large workflow instance documents or task outputs.
6. Update startup docs and deployment config for AWS credentials, table name, region, and optional S3 bucket.

## Open Design Questions

- Should `StoragePort` grow compare-and-swap methods, or should adapters handle version conflicts internally?
- Should task outputs remain embedded in `WorkflowInstance`, or should outputs become separate persisted resources?
- What retention policy should apply to completed and failed workflow instances?
- Should workflow definitions and function definitions be immutable/versioned by id, or overwritten in place as they are today?
