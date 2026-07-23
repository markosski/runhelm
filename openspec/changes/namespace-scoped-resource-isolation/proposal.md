## Why

RunHelm currently treats definitions, workflow instances, task state, events, queue entries, and worker execution as globally addressable, so reused identifiers can collide and operational paths can cross tenant boundaries. Namespace-scoped identity is required before API keys can safely select tenants in a shared deployment.

## What Changes

- Add a validated namespace value and request-scoped namespace context without global mutable state.
- Resolve public API namespace context from `RUNHELM_DEFAULT_NAMESPACE` or, when it is absent or empty, from a standard `Authorization: Bearer <api-key>` credential boundary; health checks remain namespace-independent.
- Make a configured default namespace authoritative and ignore any supplied API key when it is present.
- Require a well-formed bearer credential when no default is configured, while deliberately leaving key-to-namespace resolution as a not-implemented panic in this story.
- Scope all public and resource-specific definition, workflow-instance, task, event, queue, reconciliation, dispatch, and result operations by namespace.
- Allow only startup recovery to list workflow information without a namespace, returning namespace-qualified workflow information so unfinished instances from every namespace can be requeued safely.
- Update memory and SQL storage identities and queries so the same resource identifier can exist independently in multiple namespaces.
- Reset the SQL initial schema rather than migrating existing pre-namespace databases.
- Retain namespace identity in persisted and queued work so background and worker execution never depends on ambient request configuration.
- **BREAKING** Require `RUNHELM_DEFAULT_NAMESPACE` for usable single-tenant deployments until API-key resolution is implemented, and require SQL databases to be recreated with the namespace-aware initial schema.

## Capabilities

### New Capabilities

- `namespace-resource-isolation`: Namespace validation, request selection, resource ownership, storage isolation, and failure-closed behavior.

### Modified Capabilities

- `workflow-dataflow-engine`: Definition and workflow identity, API reads, event-backed persistence, listings, and storage adapter behavior become namespace-scoped.
- `workflow-resume`: Queue scheduling, pause/resume, retries, recovery, and reconciliation retain and enforce namespace identity.
- `task-dispatch`: Engine-to-dispatcher task payloads carry the namespace that owns the workflow execution.
- `task-dispatcher`: Pending dispatches, active leases, and worker results retain namespace identity and cannot collide across namespaces.
- `worker-pool-ipc`: Worker claim and task-result paths preserve the namespace of dispatched work through asynchronous execution.

## Impact

This affects the public API router and handlers, workflow and function services, orchestrator and engine, queue and task-dispatch contracts, startup/background reconciliation, worker endpoints, every active storage adapter, the SQL initial schema, tests, OpenSpec requirements, and developer-facing configuration and migration documentation. Storage-facing workflow information gains namespace ownership for recovery, while public definition and workflow-list bodies omit namespace routing fields because namespace is derived from request context rather than user-controlled resource payloads.
