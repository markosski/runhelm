## Context

Resource identifiers currently stand alone throughout HTTP handlers, services, storage, the workflow queue, startup recovery, and worker dispatch. The memory adapter keys maps by ID, and SQL uses globally unique IDs and ID-only relationships. That makes a namespace filter at the API insufficient: every identity-bearing boundary must carry the same namespace.

For this change, a deployment can actually select only one usable namespace through `RUNHELM_DEFAULT_NAMESPACE`. The no-default request path validates a bearer credential and reaches a resolver boundary that deliberately remains unimplemented. API-key storage, validation, and namespace lookup are follow-up work. Startup recovery is independent of request namespace selection and discovers unfinished work directly from storage.

## Goals / Non-Goals

**Goals:**

- Treat `(namespace, resource ID)` as the identity of definitions, workflow instances, tasks, verifier state, events, queue entries, and execution work.
- Resolve namespace once at the public HTTP boundary and propagate it explicitly without ambient mutable state.
- Make public point reads, listings, mutations, queue operations, dispatch, and result handling fail closed within one namespace while preserving namespace ownership during cross-namespace startup discovery.
- Permit identical definition and workflow-instance IDs in different namespaces in every storage adapter.
- Preserve namespace in asynchronous contracts so execution does not consult environment or request state later.

**Non-Goals:**

- Issuing, persisting, hashing, validating, revoking, or mapping API keys.
- Adding a tenant namespace catalog or general cross-namespace administration API; startup recovery discovers only unfinished workflow summaries through its dedicated unscoped listing mode.
- Per-namespace scheduling fairness, concurrency quotas, queue capacity, or worker pools.
- Adding namespace fields to public workflow or function definition payloads.
- Migrating data from the existing SQL schema.
- Providing authorization roles within a namespace.

## Decisions

### Namespace is a validated UUID string

Add a small `Namespace` string value type shared by core, ports, adapters, and API code. Its content must be a valid UUID in canonical hyphenated form. It is serializable as a string for internal asynchronous contracts and exposes the validated string through an accessor/display implementation.

Validation happens when the namespace resolver evaluates the configured default for a request. An empty or whitespace-only default is treated as absent; a non-empty value that is not a canonical hyphenated UUID string fails namespace resolution before any resource action. Resource payload deserialization never constructs a namespace.

Alternatives considered: unrestricted strings and DNS labels make namespace allocation dependent on naming conventions and permit identifiers with inconsistent representations. UUIDs provide a fixed, unambiguous identity format across configuration, storage, and asynchronous contracts.

### Namespace selection is an HTTP boundary with default precedence

Create a concrete `NamespaceResolver` behind an injectable `NamespaceResolverPort`. The concrete resolver owns the shared `StoragePort`, while its single `resolve` operation receives only an optional validated bearer credential and checks `RUNHELM_DEFAULT_NAMESPACE` before deciding whether storage-backed API-key resolution is needed. Public resource handlers use a request extractor backed by that resolver in router state:

1. If `RUNHELM_DEFAULT_NAMESPACE` contains a valid non-empty namespace, return it without using the presented credential.
2. Otherwise require exactly one `Authorization: Bearer <api-key>` credential and return `401 Unauthorized` for missing, malformed, or empty credentials.
3. Otherwise use the credential and resolver-owned storage port in the API-key path. That path deliberately invokes `todo!()` in this story; it never chooses a fallback namespace.

The health handler does not use the extractor. Worker registration and heartbeat remain deployment-level operations. Worker task claim/result routes obtain namespace from dispatch data rather than public-request namespace selection.

Alternatives considered: `X-API-Key` creates a custom transport contract; router middleware would also run for namespace-independent routes unless carefully split. A typed extractor makes the dependency visible in every resource handler signature.

### Namespace is an explicit argument at synchronous boundaries

Storage, workflow/function services, orchestrator, engine, state manager, and definition-reference resolution receive `&Namespace` or an owned `Namespace` where lifetime crosses an asynchronous boundary. Domain resource bodies keep their existing shape. `StoragePort::list_workflow_info` is the sole exception: it receives `Option<&Namespace>`, where `Some(namespace)` is required for normal resource operations and `None` is reserved for startup recovery across all namespaces.

Storage-facing `WorkflowInfo` includes its owning `Namespace`, allowing an unscoped recovery page to distinguish identical workflow instance IDs and construct owned queue items. Public workflow-list response models omit this internal ownership field. Namespace-scoped pagination interprets its cursor together with the supplied namespace; cross-namespace recovery pagination also includes namespace as an ordering tie-breaker so identical IDs and timestamps remain unambiguous.

This makes namespace omission a compile-time error and avoids service objects that are permanently bound to one tenant.

### Asynchronous work uses owned composite identities

Replace ID-only queue values with a `WorkflowQueueItem { namespace, workflow_instance_id }`. Queue duplicate detection, active tracking, completion, removal, status, and purge compare the composite identity; namespace-scoped API queue operations filter or mutate only matching items.

Task dispatch includes the owning namespace in `TaskDispatch`. Workers echo it in `WorkerTaskResult`, and result completion verifies it against the in-flight dispatch before delivering a result. Workspace/session identity derivation includes namespace so identical workflow IDs cannot share worker-local state.

Alternatives considered: reconstructing namespace from `RUNHELM_DEFAULT_NAMESPACE` in the scheduler or worker is unsafe after configuration changes and cannot support future API-key-selected namespaces.

### One orchestrator process may serve multiple namespaces

An orchestrator process may concurrently execute workflows from multiple namespaces. The workflow queue, task dispatcher, in-flight lease tracking, result waiters, and storage adapters are shared physical components whose resource identities and namespace-scoped operations retain the owning namespace. Correctness never relies on a process being assigned to only one namespace, even when a deployment configures `RUNHELM_DEFAULT_NAMESPACE` and currently selects only that namespace.

The shared workflow queue contains composite `WorkflowQueueItem` values rather than maintaining a separate queue instance per namespace. In-flight workflow checks compare `(namespace, workflow_instance_id)`, so the same workflow instance ID may execute independently in different namespaces. Dispatch IDs remain globally unique within their existing dispatch scope, while each dispatch and lease also retains namespace for ownership validation. Workers remain deployment-scoped and may claim work from different namespaces; the claimed payload, not worker configuration, determines the namespace.

Queue capacity and workflow concurrency are deployment-wide in this change. One namespace can therefore consume shared scheduling capacity. Per-namespace queues, quotas, weighted fairness, and dedicated worker pools are deferred until tenant scheduling policy is required. The composite queue contract permits replacing FIFO selection with a namespace-aware scheduler later without changing workflow ownership identity.

A dedicated orchestrator deployment per namespace remains a supported operational topology for stronger resource or process isolation, but it is not the tenancy correctness boundary. Such a deployment uses the same namespace-qualified contracts and storage rules as a shared process.

Alternatives considered: separate queue and in-flight component instances per namespace add lifecycle, recovery, notification, and worker-routing complexity before fairness requirements exist. Requiring one process per namespace moves tenant routing into deployment infrastructure, prevents a shared API-key-selected service, and risks hiding unscoped internal contracts behind process configuration.

### Every storage adapter encodes composite ownership natively

- Memory maps use `(Namespace, String)` composite keys and namespace-filtered collections.
- SQL adds a non-null string `namespace` column to workflow definitions, function definitions, workflow instances, tasks, verifier state, and events. The stored value is the canonical hyphenated UUID string. Primary/unique keys become composite, child tables carry namespace in composite foreign keys, and every join/filter/index begins with or constrains namespace. The initial schema is reset and includes description metadata directly; no upgrade migration is provided.

Optimistic workflow version conflicts are evaluated against the namespaced instance identity. Definition last-invoked projections update only the definition in the workflow's namespace.

### Startup recovery deliberately discovers work across namespaces

At process startup, task synchronization and active-instance requeue call `list_workflow_info(None, ...)` to page through unfinished workflow information across all namespaces. Every returned `WorkflowInfo` carries its namespace, and all subsequent snapshot reads, state transitions, task synchronization, and queue operations use that explicit namespace. Recovery does not depend on `RUNHELM_DEFAULT_NAMESPACE` and runs even when no default is configured.

The unscoped option is a narrow privileged exception for recovery discovery, not a general resource-access mode. Public handlers and workflow services always call `list_workflow_info(Some(namespace), ...)`, and all other storage operations require a namespace. SQL uses an indexed unfinished-status query rather than fetching complete tenant resources merely to filter them in application memory. Memory storage may iterate its in-process summaries for the equivalent test contract.

Bulk control and lost-host reconciliation remain namespace-scoped entry points. Once recovery discovers an owned workflow identity, it never drops the namespace or performs later operations through an unscoped lookup.

### API absence remains indistinguishable across namespaces

A point read or mutation for an ID owned by another namespace behaves exactly like an unknown ID in the selected namespace. Lists and queue status contain only selected-namespace entries. No response reveals that the same ID exists elsewhere.

## Risks / Trade-offs

- [Large signature fan-out across core and tests] → Introduce the namespace type first, then let compiler errors identify every identity-bearing call site; avoid compatibility overloads that silently select a namespace.
- [A missed SQL predicate could leak tenant data] → Add identical-ID isolation tests per adapter covering point reads, lists, mutations, events, tasks, optimistic commits, and definition projections.
- [The deliberate resolver panic can terminate a request task or process depending on panic configuration] → Keep it isolated behind the resolver interface, test it at that boundary, and document that deployments must configure the default namespace.
- [An ordinary caller could accidentally request cross-namespace workflow information] → Document `None` as recovery-only, keep direct recovery discovery inside the orchestrator startup path, require `Some(namespace)` in service APIs, and test that public operations never use the unscoped form.
- [Cross-namespace recovery could be inefficient] → Add status-oriented SQL indexing and page namespace-qualified summaries rather than loading complete workflows globally.
- [Schema reset discards existing SQL data] → Treat the release as a destructive schema boundary, require database recreation, and document backup/rollback expectations.
- [Adding namespace to worker payloads is a protocol break] → Update orchestrator and worker contracts together and cover claim/result round trips in integration tests.
- [A namespace can consume all shared queue or concurrency capacity] → Treat capacity as deployment-wide for this change; introduce explicit namespace scheduling policy before promising tenant fairness or quotas.

## Migration Plan

1. Stop the orchestrator and workers.
2. Back up any state needed for rollback; this change does not import it.
3. Configure `RUNHELM_DEFAULT_NAMESPACE` with a UUID (for example, `550e8400-e29b-41d4-a716-446655440000`).
4. Recreate the SQL database.
5. Deploy matching orchestrator and worker versions together.
6. Re-register definitions and start new workflow instances in the configured namespace.

Rollback requires redeploying the prior binaries and restoring the prior SQL database. Namespaced state is not backward compatible.

## Open Questions

None for this story. API-key resolution remains an intentionally deferred follow-up design.
