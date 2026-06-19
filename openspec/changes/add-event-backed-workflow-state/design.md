## Context

The current orchestrator treats `WorkflowInstance` as both the current-state read model and the write model. The engine mutates task status, workflow status, verifier state, input mapping, output data, and satisfaction fields directly, then persists the whole instance through `StoragePort::save_workflow_instance`.

That approach is practical while storage is in-memory, but it weakens observability of how a workflow reached its current state. RunHelm benefits from a middle ground before adopting full event sourcing: persist explicit events for audit/debugging while still persisting snapshots for cheap reads and simple list operations.

## Goals / Non-Goals

**Goals:**

- Represent workflow instance state changes as ordered domain events.
- Keep event interpretation in core/domain code, outside storage adapters.
- Persist event batches and the resulting `WorkflowInstance` snapshot.
- Continue serving current workflow reads from snapshots.
- Make storage-level workflow list reads return lightweight summaries instead of full workflow instances.
- Refactor workflow state changes incrementally without changing public API response shapes.
- Make the design compatible with future durable storage and full event sourcing.

**Non-Goals:**

- Rebuilding `WorkflowInstance` from the event log on every read.
- Adding a durable database adapter in this change.
- Adding snapshot checkpoints, event version migration, or optimistic concurrency in the first implementation.
- Changing workflow status or task result response DTOs.
- Returning full workflow instances from storage list methods.
- Making storage adapters responsible for applying workflow events.

## Decisions

### Use event-backed snapshots instead of full event sourcing

The first implementation should append workflow instance events and save the resulting snapshot. Reads continue to use snapshots.

This gives RunHelm transition history without paying the complexity cost of replay-based reads, projections, checkpointing, and event-version migration before durable storage exists.

Alternatives considered:

- Full event sourcing immediately: provides stronger history semantics, but is too much complexity while the only storage adapter is in-memory.
- Snapshot-only persistence: remains simpler, but does not improve audit/debug visibility or prepare the domain model for durable storage.

### Keep reducers in core/domain code

Workflow event processing should be implemented as pure reducer functions that apply one event or an ordered event slice to a `WorkflowInstance`.

Storage adapters should only persist raw events and snapshots. A memory, SQLite, or Postgres adapter should not know what `TaskCompleted`, `VerifierRejected`, or `StartupRecoveryApplied` means.

Alternatives considered:

- Let storage append events and mutate snapshots: centralizes persistence mechanics, but leaks domain rules into adapters and makes storage implementations harder to test and swap.

### Append events in ordered batches

The storage interface should accept a vector of events for a single workflow transition decision. Ordering is meaningful and reducer application order must match persisted order.

Empty event batches should be rejected by the core state store before storage is called.

This supports transitions where one engine decision creates multiple state changes, such as verifier rejection marking a generation unsatisfied and materializing the next generation.

Alternatives considered:

- Append one event at a time: simpler signature, but risks partially persisted logical transitions or requires a separate transaction/session API.

### Add a core workflow instance state store

A core-level store or repository should own the write sequence:

1. Load the current snapshot.
2. Apply the event batch through the reducer.
3. Append the raw events through storage.
4. Save the updated snapshot through storage.
5. Let storage update the lightweight `WorkflowInfo` projection from that already-reduced snapshot.
6. Return the updated snapshot.

Storage may maintain summary/index data as part of snapshot persistence, but it should derive that data from the updated `WorkflowInstance`, not by interpreting the event payloads. For in-memory storage this is straightforward. For durable storage, the adapter should eventually make event append, snapshot save, and summary projection update atomic in one database transaction without taking ownership of reducer logic.

### Keep public reads snapshot-backed

`get_workflow_instance`, workflow status reports, and task result lookups should continue reading from the saved snapshot. Workflow list queries and active workflow discovery should read lightweight summary rows that are maintained from the latest saved snapshot, so callers do not load full workflow snapshots for list views.

Event replay is useful for audit and debugging, but should not become the normal read path until there is a concrete product or durability reason to pay that cost.

### Replace full-instance list methods with filtered summaries

`StoragePort` should expose one workflow instance listing method that accepts a workflow state filter and returns lightweight summary rows `WorkflowInfo` instead of full `WorkflowInstance` values. The main value of this model is efficient retrieval of past and active workflow information: list calls should not load each full workflow snapshot to assemble `WorkflowInfo`; storage should maintain this summary data separately from the full snapshot, derived from the latest saved snapshot when workflow state is written.

The summary should include the fields needed for list and scheduler decisions without loading task inputs, outputs, verifier history, or full task maps:

- instance ID
- workflow definition ID
- created time when available
- completed time when available
- current workflow state
- dynamic workflow - true/false (whether or not workflow may produce more tasks as it progresses, based on control flow)
- total task count
- completed task count

The state filter should support the existing list use cases, including all instances, a specific workflow status, and active instances. Active filtering should continue to include instances that are pending/running or otherwise have pending/running task or verifier state according to summary data derived from the latest saved snapshot.

`get_workflow_instance` should be the only storage read that retrieves a full `WorkflowInstance`.

Alternatives considered:

- Keep `list_workflow_instances` and `list_active_workflow_instances` returning full instances: simpler migration, but encourages broad snapshot loading and exposes more state than list callers need.
- Add separate summary methods for all and active lists: avoids changing existing method intent, but keeps duplicated storage surface area.

## Event Model Direction

The initial event enum should cover the existing workflow state transitions without trying to model every future field:

- workflow lifecycle: created, started, completed, failed, input needed
- task attempt lifecycle: materialized, started, completed, failed, input needed
- task metadata changes: input mapping, satisfaction, verifier metadata
- verifier state changes: state created, feedback recorded, accepted, exhausted, failed
- recovery: running workflow/task reset to pending after orchestrator startup

Event payloads should carry the data needed for the reducer to produce the same current `WorkflowInstance` snapshot that the direct mutation code produces today.

## Migration Plan

1. Introduce `WorkflowInstanceEvent` and reducer tests without changing runtime behavior.
2. Add storage event append, snapshot persistence, and filtered summary listing methods.
3. Add a core workflow instance state store that appends event batches and saves snapshots.
4. Update `MemoryStorage` to store raw event logs and snapshots without interpreting event payloads.
5. Replace full-instance workflow list callers with summary-list callers where full snapshots are not needed.
6. Refactor a narrow first path, such as workflow instance creation or startup recovery, to use event batches.
7. Refactor engine task/verifier transitions incrementally, preserving existing API behavior and tests.
8. Document the event-backed snapshot model in `docs/`.

Rollback is straightforward while reads remain snapshot-backed: direct snapshot persistence can be restored and the unused event log ignored.

## Risks / Trade-offs

- Events can duplicate data already present in snapshots, increasing storage volume.
- Summary counters such as total task count and completed task count can become stale if they are computed outside the saved snapshot; compute them from the saved snapshot at write/projection time or directly in the adapter from the snapshot.
- Without durable transactions, event append and snapshot save can diverge in future non-memory adapters if not designed carefully.
- Event schemas can ossify too early; keep initial payloads close to current domain state and add versioning only when durable replay needs it.
- Partial refactors can leave mixed direct-mutation and event-backed paths; tests should make this visible and the tasks should keep the transition scoped.
