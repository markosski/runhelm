## Context

The orchestrator currently constructs `MemoryStorage` at startup and uses it through `StoragePort`. This keeps tests and local development simple, but it means workflow definitions, reusable function definitions, workflow instances, and task results are process-local and disappear on restart.

`StoragePort` exposes whole-resource persistence operations:

- read workflow definitions, function definitions, and workflow instances by id
- list all workflow instances
- list active workflow instances for startup recovery and requeueing
- save workflow definitions, function definitions, and workflow instances
- delete reusable function definitions

The active-instance listing is the most important query constraint. The in-memory adapter computes activity from workflow status, nested task statuses, and verifier state. A cloud adapter should persist an indexed active flag so startup recovery does not scan every stored workflow document.

## Goals / Non-Goals

**Goals:**

- Provide a durable cloud `StoragePort` adapter that satisfies the current port contract.
- Store queryable state and indexes in DynamoDB.
- Store document-shaped resources and large JSON payloads in S3.
- Preserve memory storage as the default local adapter.
- Allow runtime selection of storage backend through configuration.
- Keep workflow, function, and API service behavior compatible with existing callers.
- Document required AWS resources, IAM actions, configuration, and operational behavior.

**Non-Goals:**

- Replacing `StoragePort` with event sourcing.
- Adding multi-writer distributed workflow execution semantics beyond the current orchestrator behavior.
- Changing public API response shapes.
- Moving workspace file persistence into this adapter.
- Implementing historical audit/query views beyond the latest persisted state.

## Decisions

### Use DynamoDB for metadata and indexes

The adapter will write one DynamoDB metadata item per stored logical resource. DynamoDB will hold the resource type, id, status, active flag, updated timestamp, version, and S3 object pointer. The resource body will live in S3.

Proposed table shape:

- table name from `RUNHELM_STORAGE_DYNAMODB_TABLE`
- partition key: `pk`
- sort key: `sk`
- workflow definition item: `pk = WORKFLOW_DEF#{id}`, `sk = METADATA`
- function definition item: `pk = FUNCTION_DEF#{id}`, `sk = METADATA`
- workflow instance item: `pk = WORKFLOW_INSTANCE#{id}`, `sk = METADATA`

Workflow instance metadata should include:

- `resource_type = workflow_instance`
- `id`
- `workflow_def_id`
- `status`
- `active`
- `updated_at`
- `version`
- `document_bucket`
- `document_key`

Indexes:

- `resource_type_updated_at`: partition key `resource_type`, sort key `updated_at`, used for `list_workflow_instances`.
- `active_updated_at`: partition key `active`, sort key `updated_at`, used for `list_active_workflow_instances`.

Alternative considered: storing complete documents directly in DynamoDB. This is simpler but risks the 400 KB item limit for workflow instances with task outputs, verifier feedback, or function bodies. Keeping DynamoDB focused on metadata gives predictable query behavior and avoids item-size pressure.

### Use S3 for document bodies

The adapter will serialize domain models as JSON and store them in S3. DynamoDB remains the source of queryable metadata and stores the pointer to the current S3 object.

Proposed key layout:

- `{prefix}/workflow-defs/{id}.json`
- `{prefix}/function-defs/{id}.json`
- `{prefix}/workflow-instances/{id}.json`

The adapter should write the S3 object before updating DynamoDB metadata. Reads first load metadata from DynamoDB, then fetch and deserialize the S3 object. If metadata exists but the S3 object is missing or invalid, the operation should return an error rather than treating the resource as absent.

Alternative considered: S3-only storage with prefix listings and active marker objects. This avoids DynamoDB but makes active workflow listing and all-instance listing less direct, and it pushes index maintenance into object naming conventions.

### Derive active state on workflow instance save

`save_workflow_instance` will compute `active` from the same rules as `MemoryStorage`:

- workflow status is `Pending` or `Running`
- any task status is `Pending` or `Running`
- any verifier state is `Running`

The computed value is stored in DynamoDB so `list_active_workflow_instances` can query the `active_updated_at` index. This keeps the cloud adapter behavior aligned with existing recovery logic while avoiding document scans.

Alternative considered: query by top-level workflow status only. That would miss instances where the workflow status is not active but nested task or verifier state still requires recovery.

### Keep the current StoragePort contract for this change

The first implementation should not add compare-and-swap methods to `StoragePort`. It should behave like the memory adapter from the caller's perspective and use last-write-wins semantics. The DynamoDB `version` field should still be maintained for observability and future optimistic concurrency support.

Alternative considered: adding explicit versioned writes now. That would make conflict handling clearer, but it would also require changing core services and tests before there is a demonstrated multi-orchestrator writer requirement.

### Select storage backend at startup through configuration

`main.rs` will construct storage through a small factory instead of directly instantiating `MemoryStorage`.

Proposed environment variables:

- `RUNHELM_STORAGE_BACKEND=memory|aws`
- `RUNHELM_STORAGE_DYNAMODB_TABLE`
- `RUNHELM_STORAGE_S3_BUCKET`
- `RUNHELM_STORAGE_S3_PREFIX`
- `AWS_REGION` or standard AWS SDK region configuration

When `RUNHELM_STORAGE_BACKEND` is unset, the orchestrator should continue using memory storage. When `aws` is selected, missing table or bucket configuration should fail startup with a clear error.

Alternative considered: compile-time feature selection only. Runtime selection is more useful for container deployments and keeps one binary usable for local and cloud environments.

### Introduce AWS SDK dependencies in the orchestrator crate

The adapter will use the official Rust AWS SDK crates for DynamoDB, S3, and shared configuration. AWS clients should be injected into the adapter constructor where possible to keep tests focused and allow localstack-style integration tests later.

Unit tests should cover metadata derivation, S3 key generation, serialization round trips, and error behavior. Integration tests that require DynamoDB/S3 or LocalStack should be opt-in so the default test suite remains fast and hermetic.

## Risks / Trade-offs

- DynamoDB metadata update succeeds after S3 write, but later cleanup fails -> stale S3 objects can accumulate. Mitigation: use deterministic object keys for latest-state documents and add cleanup/reconciliation docs for future versioned-object use.
- S3 write succeeds but DynamoDB write fails -> the new document is not visible. Mitigation: write deterministic keys and allow retry to overwrite the same key before metadata is updated.
- DynamoDB metadata points to a missing or corrupted S3 object -> reads fail. Mitigation: surface this as storage corruption with context rather than returning `None`.
- Active flags can become wrong if activity rules diverge from `MemoryStorage`. Mitigation: share an `is_active_workflow_instance` helper between adapters.
- Whole-instance saves can still overwrite concurrent updates. Mitigation: keep the version attribute and defer compare-and-swap port changes until multi-writer orchestration is designed.
- AWS dependencies increase binary size and startup configuration surface. Mitigation: keep memory storage as the default and isolate AWS code in a dedicated adapter module.

## Migration Plan

1. Add the AWS storage adapter behind runtime configuration while leaving memory storage as default.
2. Provision DynamoDB table, GSIs, and S3 bucket for deployments that select the AWS backend.
3. Deploy with `RUNHELM_STORAGE_BACKEND=memory` first to verify no startup behavior changed.
4. Switch a non-production deployment to `RUNHELM_STORAGE_BACKEND=aws` and verify definition registration, workflow execution, restart recovery, and function deletion.
5. Roll back by switching `RUNHELM_STORAGE_BACKEND` to `memory` or redeploying the prior version. Existing S3/DynamoDB data remains untouched.

## Open Questions

- Should the adapter support custom DynamoDB endpoint and S3 endpoint variables for LocalStack in the first implementation?
- Should S3 object versioning be required or only recommended?
- Should completed workflow instances eventually receive a retention/TTL policy?
- Should task outputs become separate S3 documents later instead of remaining embedded in the workflow instance document?
