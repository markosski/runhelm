## 1. Namespace Foundation and HTTP Selection

- [ ] 1.1 Add the validated DNS-label `Namespace` value type, serialization support for internal contracts, and unit tests for accepted and rejected values.
- [ ] 1.2 Add immutable default-namespace configuration and a `NamespaceResolver` boundary whose API-key implementation deliberately panics as not implemented.
- [ ] 1.3 Add the public request namespace extractor with default precedence, `Authorization: Bearer` validation, `401 Unauthorized` failures, and health-check exemption tests.
- [ ] 1.4 Require namespace context in every public resource handler without adding namespace routing fields to public definition payloads.

## 2. Namespace-Scoped Storage Contract and Core Services

- [ ] 2.1 Add explicit namespace parameters to every definition and workflow operation in `StoragePort`, including point reads, lists, deletes, events, and atomic workflow commits.
- [ ] 2.2 Propagate namespace through `FunctionService`, `WorkflowService`, definition-reference resolution, and workflow immutability checks.
- [ ] 2.3 Propagate namespace through `WorkflowStateManager` and `WorkflowEngine` reads, transitions, status queries, and task dispatch calls.
- [ ] 2.4 Propagate namespace through orchestrator resource actions, isolated task invocation, pause/resume, retries, human input, and queue APIs.

## 3. Memory Storage Isolation

- [ ] 3.1 Convert memory definition, function, workflow snapshot, summary, and event keys to namespace/resource composite identities.
- [ ] 3.2 Scope memory lists, filters, pagination, deletes, optimistic commits, and definition last-invoked projections to the supplied namespace.
- [ ] 3.3 Add memory adapter tests proving identical IDs, reads, lists, mutations, events, tasks, and version conflicts remain isolated.

## 4. SQL Storage Isolation and Schema Reset

- [ ] 4.1 Reset SQL migrations to one current initial schema with namespace columns, composite primary/foreign keys, current definition metadata, and namespace-leading indexes.
- [ ] 4.2 Add namespace predicates and bindings to every SQL definition and workflow point read, write, delete, join, event/task/verifier operation, and projection update.
- [ ] 4.3 Scope SQL workflow listing, filters, ordering, pagination cursors, event pages, and optimistic version checks to namespace.
- [ ] 4.4 Add fresh-schema constraint and adapter tests proving identical IDs and all resource relationships remain isolated across namespaces.

## 5. AWS Storage Isolation

- [ ] 5.1 Encode namespace into DynamoDB definition, workflow, event, task, and list-projection partition/sort keys without cross-namespace scans or client-side filtering.
- [ ] 5.2 Encode namespace into S3 immutable payload paths and scope definition last-invoked and transactional workflow updates to the owning namespace.
- [ ] 5.3 Update AWS query, pagination, transaction, reconstruction, and stale-version paths for namespace/resource composite identity.
- [ ] 5.4 Add AWS adapter tests proving identical IDs, projections, events, tasks, objects, mutations, and conflicts remain isolated.

## 6. Queue, Recovery, and Reconciliation

- [ ] 6.1 Replace ID-only workflow queue values with owned namespace/workflow composite items and make duplicate, active, complete, remove, status, and purge behavior namespace-aware.
- [ ] 6.2 Scope startup task synchronization, active workflow discovery/requeue, bulk control, and lost-host reconciliation to an explicit namespace.
- [ ] 6.3 Run startup recovery only for the configured default namespace and log an explicit skip without global scans when no default is configured.
- [ ] 6.4 Add queue and orchestrator tests for identical IDs, namespace-scoped bulk operations, recovery, requeue, and reconciliation.

## 7. Worker Dispatch and Result Isolation

- [ ] 7.1 Add namespace to `TaskDispatchPort`, pending dispatches, active leases, claimed task payloads, and isolated execution identities.
- [ ] 7.2 Add namespace to worker task results and reject a result whose namespace differs from its active dispatch without completing that dispatch.
- [ ] 7.3 Include namespace in worker workspace and reusable Agent session identity derivation so identical workflow/task IDs cannot share local state.
- [ ] 7.4 Update the TypeScript worker claim/result protocol to retain and echo the claimed namespace without consulting worker environment.
- [ ] 7.5 Add orchestrator and worker tests for namespaced claims, concurrent identical workflow IDs, matching results, mismatched results, workspaces, and sessions.

## 8. API Integration and Failure-Closed Coverage

- [ ] 8.1 Add router/handler integration tests for default precedence, ignored authorization, missing and malformed bearer credentials, resolver invocation, and health exemption.
- [ ] 8.2 Add end-to-end public API tests proving point reads, lists, mutations, events, tasks, queue actions, retries, and human input never expose another namespace.
- [ ] 8.3 Ensure cross-namespace unknown resources use the same public response contract as absent resources and do not reveal ownership.

## 9. Documentation and Verification

- [ ] 9.1 Update website installation, API, storage, and operational documentation for `RUNHELM_DEFAULT_NAMESPACE`, bearer behavior, deferred resolver panic, startup recovery scope, SQL recreation, and fresh AWS resources/prefixes.
- [ ] 9.2 Update relevant examples and local-development configuration to set `RUNHELM_DEFAULT_NAMESPACE=default`.
- [ ] 9.3 Run Rust formatting and the complete orchestrator test suite, then run worker formatting/type checks and tests.
- [ ] 9.4 Run the website build and strict OpenSpec validation, review the diff for accidental public namespace fields or unscoped identity paths, and resolve all failures.
