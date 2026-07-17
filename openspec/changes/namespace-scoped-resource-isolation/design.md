## Context

Resource identifiers currently stand alone throughout HTTP handlers, services, storage, the workflow queue, startup recovery, and worker dispatch. The memory adapter keys maps by ID, SQL uses globally unique IDs and ID-only relationships, and AWS composes DynamoDB/S3 keys and list projections without an ownership component. That makes a namespace filter at the API insufficient: every identity-bearing boundary must carry the same namespace.

For this change, a deployment can actually select only one usable namespace through `RUNHELM_DEFAULT_NAMESPACE`. The no-default request path validates a bearer credential and reaches a resolver boundary that deliberately remains unimplemented. API-key storage, validation, namespace lookup, and a durable namespace catalog are follow-up work.

## Goals / Non-Goals

**Goals:**

- Treat `(namespace, resource ID)` as the identity of definitions, workflow instances, tasks, verifier state, events, queue entries, and execution work.
- Resolve namespace once at the public HTTP boundary and propagate it explicitly without ambient mutable state.
- Make point reads, listings, mutations, recovery, queue operations, dispatch, and result handling fail closed within one namespace.
- Permit identical definition and workflow-instance IDs in different namespaces in every storage adapter.
- Preserve namespace in asynchronous contracts so execution does not consult environment or request state later.

**Non-Goals:**

- Issuing, persisting, hashing, validating, revoking, or mapping API keys.
- Enumerating tenant namespaces for cross-namespace administration or recovery.
- Adding namespace fields to public workflow or function definition payloads.
- Migrating data from the existing SQL schema or globally keyed AWS records.
- Providing authorization roles within a namespace.

## Decisions

### Namespace is a validated domain value

Add a small `Namespace` value type shared by core, ports, adapters, and API code. It uses the DNS-label form commonly used for deployment namespaces: 1–63 lowercase ASCII letters, digits, or hyphens; it starts and ends with an alphanumeric character. It is serializable for internal asynchronous contracts and exposes its string only through an accessor/display implementation.

Validation happens when configuration or a resolver constructs the value. An empty or whitespace-only default is treated as absent; a non-empty invalid configured value fails startup. Resource payload deserialization never constructs a namespace.

Alternatives considered: unrestricted strings make storage-key composition and operational use unsafe; UUID-only namespaces are unnecessarily opaque for configured single-tenant deployments.

### Namespace selection is an HTTP boundary with default precedence

Create a `NamespaceResolver` interface that accepts the presented API-key credential and returns a `Namespace`. Public resource handlers use a request extractor backed by immutable router state:

1. If a valid non-empty `RUNHELM_DEFAULT_NAMESPACE` was configured, return it and do not inspect the authorization header.
2. Otherwise require exactly one `Authorization: Bearer <api-key>` credential and return `401 Unauthorized` for missing, malformed, or empty credentials.
3. Pass the credential to the resolver. The resolver implementation for this story deliberately invokes `todo!()`; it never chooses a fallback namespace.

The health handler does not use the extractor. Worker registration and heartbeat remain deployment-level operations. Worker task claim/result routes obtain namespace from dispatch data rather than public-request namespace selection.

Alternatives considered: `X-API-Key` creates a custom transport contract; router middleware would also run for namespace-independent routes unless carefully split. A typed extractor makes the dependency visible in every resource handler signature.

### Namespace is an explicit argument at synchronous boundaries

Storage, workflow/function services, orchestrator, engine, state manager, and definition-reference resolution receive `&Namespace` or an owned `Namespace` where lifetime crosses an asynchronous boundary. Domain resource bodies keep their existing shape. Pagination cursors remain ID/time based because a cursor is interpreted only together with the namespace supplied to the list call.

This makes namespace omission a compile-time error and avoids service objects that are permanently bound to one tenant.

### Asynchronous work uses owned composite identities

Replace ID-only queue values with a `WorkflowQueueItem { namespace, workflow_instance_id }`. Queue duplicate detection, active tracking, completion, removal, status, and purge compare the composite identity; namespace-scoped API queue operations filter or mutate only matching items.

Task dispatch includes the owning namespace in `TaskDispatch`. Workers echo it in `WorkerTaskResult`, and result completion verifies it against the in-flight dispatch before delivering a result. Workspace/session identity derivation includes namespace so identical workflow IDs cannot share worker-local state.

Alternatives considered: reconstructing namespace from `RUNHELM_DEFAULT_NAMESPACE` in the scheduler or worker is unsafe after configuration changes and cannot support future API-key-selected namespaces.

### Every storage adapter encodes composite ownership natively

- Memory maps use `(Namespace, String)` composite keys and namespace-filtered collections.
- SQL adds a non-null `namespace` column to workflow definitions, function definitions, workflow instances, tasks, verifier state, and events. Primary/unique keys become composite, child tables carry namespace in composite foreign keys, and every join/filter/index begins with or constrains namespace. The initial schema is reset and includes description metadata directly; no upgrade migration is provided.
- AWS includes an encoded namespace component in definition partitions, workflow-instance partitions, list projection partitions, task/event keys, and S3 object paths. All point reads and queries calculate a namespace-specific key; no table scan or client-side tenant filtering is introduced.

Optimistic workflow version conflicts are evaluated against the namespaced instance identity. Definition last-invoked projections update only the definition in the workflow's namespace.

### Background recovery is explicitly scoped

Startup recovery, active-instance requeue, and lost-host reconciliation accept a namespace. At process startup they run only for the configured default namespace. If no default namespace is configured, startup logs that recovery/requeue are skipped because this story has no trusted namespace catalog; it does not perform a global scan. Tests can invoke the same operations independently for multiple namespace values.

Future API-key support must introduce a trusted namespace catalog before multi-namespace startup recovery is enabled. That is preferable to adding a global resource-list operation that could leak into public code.

### API absence remains indistinguishable across namespaces

A point read or mutation for an ID owned by another namespace behaves exactly like an unknown ID in the selected namespace. Lists and queue status contain only selected-namespace entries. No response reveals that the same ID exists elsewhere.

## Risks / Trade-offs

- [Large signature fan-out across core and tests] → Introduce the namespace type first, then let compiler errors identify every identity-bearing call site; avoid compatibility overloads that silently select a namespace.
- [A missed SQL predicate or AWS key component could leak tenant data] → Add identical-ID isolation tests per adapter covering point reads, lists, mutations, events, tasks, optimistic commits, and definition projections.
- [The deliberate resolver panic can terminate a request task or process depending on panic configuration] → Keep it isolated behind the resolver interface, test it at that boundary, and document that deployments must configure the default namespace.
- [No-default startup cannot recover persisted tenant work] → Skip recovery explicitly and log the reason; do not scan globally. API-key support must add namespace discovery before enabling that deployment mode.
- [Schema reset discards existing SQL data] → Treat the release as a destructive schema boundary, require database recreation, and document backup/rollback expectations.
- [Existing AWS records remain globally keyed and unreachable] → Require fresh tables/prefixes or explicit operator cleanup; do not mix old and namespaced identities.
- [Adding namespace to worker payloads is a protocol break] → Update orchestrator and worker contracts together and cover claim/result round trips in integration tests.

## Migration Plan

1. Stop the orchestrator and workers.
2. Back up any state needed for rollback; this change does not import it.
3. Configure a valid `RUNHELM_DEFAULT_NAMESPACE` (normally `default`).
4. Recreate the SQL database, or provision fresh AWS DynamoDB tables and an unused S3 prefix.
5. Deploy matching orchestrator and worker versions together.
6. Re-register definitions and start new workflow instances in the configured namespace.

Rollback requires redeploying the prior binaries and restoring the prior SQL database or AWS table/prefix configuration. Namespaced state is not backward compatible.

## Open Questions

None for this story. API-key resolution and trusted namespace enumeration are intentionally deferred follow-up designs.
