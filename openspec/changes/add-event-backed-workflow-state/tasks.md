## 1. Domain Event Model

- [ ] 1.1 Define `WorkflowInstanceEvent` for workflow lifecycle, task attempt lifecycle, verifier state, failure/completion, and startup recovery transitions
- [ ] 1.2 Include event payload data needed to reproduce the current `WorkflowInstance` snapshot behavior
- [ ] 1.3 Add conversion or storage-friendly wrappers only where persistence requires them

## 2. Core Reducer

- [ ] 2.1 Add pure reducer functions for applying one event to a mutable `WorkflowInstance`
- [ ] 2.2 Add reducer support for applying an ordered event slice
- [ ] 2.3 Reject invalid reducer inputs that would corrupt instance state
- [ ] 2.4 Add unit tests for workflow lifecycle, task attempt, verifier, failure, completion, and recovery events

## 3. Core State Store

- [ ] 3.1 Add a core workflow instance state store or repository around `StoragePort`
- [ ] 3.2 Implement event batch validation, including rejecting empty batches
- [ ] 3.3 Implement load snapshot, reduce events, append events, save snapshot, update summary projection, and return updated snapshot flow
- [ ] 3.4 Keep event interpretation inside the reducer, not inside storage adapters

## 4. Storage Port And Memory Adapter

- [ ] 4.1 Extend `StoragePort` with workflow instance event append and snapshot save operations
- [ ] 4.2 Replace `list_workflow_instances` and `list_active_workflow_instances` with one filtered workflow instance summary listing method
- [ ] 4.3 Define workflow instance summary fields: instance ID, workflow definition ID, created time, completed time, current state, dynamic workflow flag, total task count, and completed task count
- [ ] 4.4 Preserve snapshot-backed `get_workflow_instance` as the only full-instance storage read
- [ ] 4.5 Update `MemoryStorage` to store raw workflow instance event batches or ordered logs
- [ ] 4.6 Add memory storage tests proving events are persisted without adapter-side event reduction
- [ ] 4.7 Add memory storage tests proving summary rows are maintained from already-reduced snapshots, not by interpreting events
- [ ] 4.8 Add memory storage tests for all, status-filtered, and active summary list queries

## 5. Runtime Adoption

- [ ] 5.1 Refactor workflow instance creation to use event-backed snapshot updates
- [ ] 5.2 Refactor orchestrator startup recovery to use event-backed snapshot updates
- [ ] 5.3 Refactor a focused subset of workflow engine task lifecycle transitions to use event batches
- [ ] 5.4 Update workflow list and active workflow discovery callers to use summary listing unless they need full instances
- [ ] 5.5 Keep public API response shapes stable during the migration

## 6. Verification And Documentation

- [ ] 6.1 Run existing orchestrator tests and update expectations only when event-backed behavior intentionally changes internals
- [ ] 6.2 Add tests for ordered event batch application
- [ ] 6.3 Add tests for snapshot-backed reads after event append
- [ ] 6.4 Add tests that `get_workflow_instance` remains the only storage path returning full workflow instance state
- [ ] 6.5 Update `docs/core-service-boundaries.md` or add a dedicated `docs/` page explaining the event-backed snapshot and summary listing model
