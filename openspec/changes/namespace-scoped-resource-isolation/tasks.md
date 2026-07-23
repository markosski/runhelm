## 1. Namespace Foundation and HTTP Selection

- [x] 1.1 Add the UUID-string `Namespace` value type, string serialization support for internal contracts, and unit tests for accepted and rejected UUID strings.
- [x] 1.2 Add a testable `NamespaceResolver` boundary that owns `StoragePort`, checks `RUNHELM_DEFAULT_NAMESPACE`, and deliberately panics for unimplemented storage-backed API-key resolution.
- [x] 1.3 Add the public request namespace extractor with default precedence, `Authorization: Bearer` validation, `401 Unauthorized` failures, and health-check exemption tests.
- [x] 1.4 Require namespace context in every public resource handler without adding namespace routing fields to public definition payloads.

## 2. Namespace-Scoped Storage Contract and Core Services

- [ ] 2.1 Add explicit namespace parameters to every definition and workflow operation in `StoragePort`, including point reads, lists, deletes, events, and atomic workflow commits; make `list_workflow_info` alone accept `Option<&Namespace>`, with `None` reserved for startup recovery.
- [ ] 2.2 Add namespace ownership to storage-facing `WorkflowInfo` and cross-namespace pagination identity while keeping namespace out of public workflow-list response bodies.
- [ ] 2.3 Propagate namespace through `FunctionService`, `WorkflowService`, definition-reference resolution, and workflow immutability checks.
- [ ] 2.4 Propagate namespace through `WorkflowStateManager` and `WorkflowEngine` reads, transitions, status queries, and task dispatch calls.
- [ ] 2.5 Propagate namespace through orchestrator resource actions, isolated task invocation, pause/resume, retries, human input, and queue APIs.

## 3. Memory Storage Isolation

- [ ] 3.1 Convert memory definition, function, workflow snapshot, summary, and event keys to namespace/resource composite identities.
- [ ] 3.2 Scope memory lists, filters, pagination, deletes, optimistic commits, and definition last-invoked projections to the supplied namespace, while supporting recovery-only workflow-info listing without one.
- [ ] 3.3 Add memory adapter tests proving identical IDs, reads, lists, mutations, events, tasks, and version conflicts remain isolated.

## 4. SQL Storage Isolation and Schema Reset

- [ ] 4.1 Reset SQL migrations to one current initial schema with namespace columns, composite primary/foreign keys, current definition metadata, and namespace-leading indexes.
- [ ] 4.2 Add namespace predicates and bindings to every SQL definition and workflow point read, write, delete, join, event/task/verifier operation, and projection update.
- [ ] 4.3 Scope SQL workflow listing, filters, ordering, pagination cursors, event pages, and optimistic version checks to namespace, and add an indexed paginated recovery query that returns unfinished workflow information across namespaces.
- [ ] 4.4 Add fresh-schema constraint and adapter tests proving identical IDs and all resource relationships remain isolated across namespaces.

## 5. Queue, Recovery, and Reconciliation

- [ ] 5.1 Replace ID-only workflow queue values with owned namespace/workflow composite items in one shared queue and make duplicate, active, complete, remove, status, and purge behavior namespace-aware.
- [ ] 5.2 Keep bulk control and lost-host reconciliation namespace-scoped, and retain the discovered namespace through every recovery read, transition, and queue action.
- [ ] 5.3 Make startup task synchronization and active workflow requeue call `list_workflow_info(None, ...)`, recover unfinished instances from every namespace regardless of default configuration, and enqueue namespace/workflow composite items.
- [ ] 5.4 Add queue and orchestrator tests for identical IDs, namespace-scoped bulk operations, cross-namespace recovery with and without a configured default, requeue, and reconciliation.

## 6. Worker Dispatch and Result Isolation

- [ ] 6.1 Add namespace to `TaskDispatchPort`, shared pending dispatches, active leases, claimed task payloads, and isolated execution identities while keeping workers deployment-scoped.
- [ ] 6.2 Add namespace to worker task results and reject a result whose namespace differs from its active dispatch without completing that dispatch.
- [ ] 6.3 Include namespace in worker workspace and reusable Agent session identity derivation so identical workflow/task IDs cannot share local state.
- [ ] 6.4 Update the TypeScript worker claim/result protocol to retain and echo the claimed namespace without consulting worker environment.
- [ ] 6.5 Add orchestrator and worker tests for namespaced claims, concurrent identical workflow IDs, matching results, mismatched results, workspaces, and sessions.

## 7. API Integration and Failure-Closed Coverage

- [ ] 7.1 Add router/handler integration tests for default precedence, ignored authorization, missing and malformed bearer credentials, resolver invocation, and health exemption.
- [ ] 7.2 Add end-to-end public API tests proving point reads, lists, mutations, events, tasks, queue actions, retries, and human input never expose another namespace.
- [ ] 7.3 Ensure cross-namespace unknown resources use the same public response contract as absent resources and do not reveal ownership.

## 8. Documentation and Verification

- [ ] 8.1 Update website installation, API, storage, and operational documentation for `RUNHELM_DEFAULT_NAMESPACE`, bearer behavior, deferred resolver panic, cross-namespace startup recovery, and SQL recreation.
- [x] 8.2 Update relevant examples and local-development configuration to set `RUNHELM_DEFAULT_NAMESPACE` to a UUID.
- [ ] 8.3 Run Rust formatting and the complete orchestrator test suite, then run worker formatting/type checks and tests.
- [ ] 8.4 Run the website build and strict OpenSpec validation, review the diff for accidental public namespace fields or unscoped identity paths, and resolve all failures.
