# Event-Backed Workflow State

RunHelm is considering an event-backed snapshot model for workflow instance state changes. The proposal is tracked in `openspec/changes/add-event-backed-workflow-state/`.

The intended middle ground is:

- workflow state changes are represented as ordered domain events
- core reducer logic applies events to produce the next `WorkflowInstance` snapshot
- a core workflow state manager coordinates applying the reducer and asking storage to commit timestamped event records with the updated snapshot
- state manager callers can either commit by workflow instance ID, which loads the current snapshot, or commit against an already-held `WorkflowInstance`, avoiding redundant snapshot reads
- runtime workflow, task, and verifier state transitions commit through the workflow state manager rather than direct snapshot saves
- storage adapters persist event records and snapshots, and may maintain lightweight summary rows from committed snapshots, but do not interpret event meaning
- current workflow reads continue to use snapshots rather than replaying events
- workflow instance lists use one filtered summary query instead of returning full workflow instances
- workflow definitions can be overwritten only until the first workflow instance is created for that definition ID

This is not full event sourcing. Full replay-based reads, snapshot checkpoints, optimistic concurrency, event version migrations, and durable database transaction details remain future work until RunHelm needs durable storage semantics.

The main boundary is that event processing belongs in core/domain code. Storage should provide persistence mechanics only, so memory, SQLite, or Postgres adapters can remain swappable without duplicating workflow transition rules. The core workflow state manager assigns `created_time` when wrapping events in `WorkflowEventRecord`; storage persists those records without interpreting the payload. Storage adapters may build and persist `WorkflowInfo` summary rows from the committed `WorkflowInstance` snapshot without pushing optimization-only fields back into the domain snapshot. Summary lifecycle timestamps are derived from persisted workflow event record timestamps: creation from the first event batch, modification from the latest event batch, and completion from the first terminal reduced snapshot. The storage port no longer exposes a direct runtime snapshot-save operation for workflow instances.

The intended storage read shape is also narrower than the current snapshot API. `get_workflow_instance` remains the only storage operation that returns full workflow state. List calls should return lightweight summaries with identity, lifecycle timestamps when available, current state, and task completion counts. `list_workflow_info` accepts zero or more summary filters plus a required page request; an empty filter list matches all summaries, status filters match any listed status, workflow definition ID filters match one definition, and multiple filters are combined with AND semantics. Results are sorted by most recent `modified_at_epoch_ms` first, with workflow instance ID as a deterministic tie-breaker, and callers page with an opaque cursor derived from that ordering. Active workflow discovery uses the same filter and pagination mechanism as list views, requesting pending and running statuses.

Workflow definition registration remains mutable only before execution history exists. Once any workflow instance summary exists for a workflow definition ID, registering another definition with that normalized ID is rejected so historical instances cannot be reinterpreted against a newer graph. Callers should create a new workflow definition ID for versioned workflow changes after instances exist.

The public API exposes workflow events separately from the current-state workflow read:

- `GET /workflows/{id}` returns the current workflow status snapshot view.
- `GET /workflows` returns a bounded page of `WorkflowInfo` list entries with workflow identity, lifecycle timestamps when available, current state, task counts, and a `next_cursor` when more results are available.
- `GET /workflows/{id}/events` returns timestamped `WorkflowEventRecord` entries for audit and debugging.
