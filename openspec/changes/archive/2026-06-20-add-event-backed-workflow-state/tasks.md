## 1. Domain Event Model

- [x] 1.1 Define `WorkflowInstanceEvent` for workflow lifecycle, task attempt lifecycle, verifier state, failure/completion, and startup recovery transitions
- [x] 1.2 Include event payload data needed to reproduce the current `WorkflowInstance` snapshot behavior and persist events through `WorkflowEventRecord` with `created_time`
- [x] 1.3 Add conversion or storage-friendly wrappers only where persistence requires them

## 2. Core Reducer

- [x] 2.1 Add pure reducer functions for applying one event to a mutable `WorkflowInstance`
- [x] 2.2 Add reducer support for applying an ordered event slice
- [x] 2.3 Reject invalid reducer inputs that would corrupt instance state
- [x] 2.4 Add unit tests for workflow lifecycle, task attempt, verifier, failure, completion, and recovery events

## 3. Core State Manager

- [x] 3.1 Add a core workflow state manager around `StoragePort`
- [x] 3.2 Implement event batch validation, including rejecting empty batches
- [x] 3.3 Implement load snapshot, reduce events, commit event records with snapshot, and return updated snapshot flow
- [x] 3.4 Keep event interpretation inside the reducer, not inside storage adapters

## 4. Storage Port And Memory Adapter

- [x] 4.1 Extend `StoragePort` with one workflow transition commit operation for event records and snapshot state
- [x] 4.2 Replace `list_workflow_instances` and `list_active_workflow_instances` with one filtered workflow instance summary listing method
- [x] 4.3 Define workflow instance summary fields: instance ID, workflow definition ID, created time, completed time, current state, total task count, and completed task count
- [x] 4.4 Preserve snapshot-backed `get_workflow_instance` as the only full-instance storage read
- [x] 4.5 Update `MemoryStorage` to store raw workflow instance event batches or ordered logs
- [x] 4.6 Add memory storage tests proving events are persisted without adapter-side event reduction
- [x] 4.7 Add memory storage tests proving summary rows are maintained from already-reduced snapshots, not by interpreting events
- [x] 4.8 Add memory storage tests for all, status-filtered, and active summary list queries

## 5. Runtime Adoption

- [x] 5.1 Refactor workflow instance creation to use event-backed snapshot updates
- [x] 5.2 Refactor orchestrator startup recovery to use event-backed snapshot updates
- [x] 5.3 Refactor workflow engine task and verifier lifecycle transitions to use event batches
- [x] 5.4 Update workflow list and active workflow discovery callers to use summary listing unless they need full instances
- [x] 5.5 Keep workflow status and task result API response shapes stable while exposing `WorkflowInfo` for workflow lists

## 6. Verification And Documentation

- [x] 6.1 Run existing orchestrator tests and update expectations only when event-backed behavior intentionally changes internals
- [x] 6.2 Add tests for ordered event batch application
- [x] 6.3 Add tests for snapshot-backed reads after event append
- [x] 6.4 Add tests that `get_workflow_instance` remains the only storage path returning full workflow instance state
- [x] 6.5 Update `docs/core-service-boundaries.md` or add a dedicated `docs/` page explaining the event-backed snapshot and summary listing model
