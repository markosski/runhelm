## Why

RunHelm currently records workflow execution progress by directly mutating a full `WorkflowInstance` snapshot and saving that snapshot after meaningful transitions. This is simple, but it leaves no durable transition history and makes future auditability, debugging, and durable storage semantics harder to add cleanly.

## What Changes

- Add workflow instance events for workflow lifecycle, task attempt, verifier, failure, completion, and recovery transitions.
- Add pure core/domain reducer logic that applies ordered workflow instance events to a `WorkflowInstance`.
- Add a core-level workflow state manager that applies event batches to the current snapshot and commits event records together with the resulting snapshot, loading the snapshot only when the caller does not already have it.
- Extend storage interfaces with one workflow transition commit operation while keeping storage adapters free of workflow transition semantics.
- Replace full-instance list methods with a single filtered workflow instance summary listing method backed by snapshots.
- Keep full workflow instance retrieval limited to `get_workflow_instance`.
- Defer full event sourcing, replay-based reads, snapshot checkpoints, optimistic concurrency, and durable database transaction details until durable storage requires them.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workflow-dataflow-engine`: Workflow instance state transitions become event-backed while current-state reads remain snapshot-backed, and workflow list reads return lightweight summaries through one filtered listing operation.

## Impact

- Affects the orchestrator storage port, memory storage adapter, workflow engine state mutation paths, workflow service creation paths, startup recovery paths, workflow list API DTO, and tests.
- Adds a workflow instance event model and reducer to core/domain code.
- Preserves full workflow status and task result read behavior while using `WorkflowInfo` for lightweight workflow list responses.
- Does not require a durable database or full event replay for reads.
