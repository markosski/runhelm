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

This is not full event sourcing. Full replay-based reads, snapshot checkpoints, optimistic concurrency, event version migrations, and durable database transaction details remain future work until RunHelm needs durable storage semantics.

The main boundary is that event processing belongs in core/domain code. Storage should provide persistence mechanics only, so memory, SQLite, or Postgres adapters can remain swappable without duplicating workflow transition rules. The core workflow state manager assigns `created_time` when wrapping events in `WorkflowEventRecord`; storage persists those records without interpreting the payload. Storage adapters may build and persist `WorkflowInfo` summary rows from the committed `WorkflowInstance` snapshot without pushing optimization-only fields back into the domain snapshot. The storage port no longer exposes a direct runtime snapshot-save operation for workflow instances.

The intended storage read shape is also narrower than the current snapshot API. `get_workflow_instance` remains the only storage operation that returns full workflow state. List calls should return lightweight summaries with identity, lifecycle timestamps when available, current state, and task completion counts. Active workflow discovery uses the same status filter mechanism as list views, requesting pending and running statuses.

The public API exposes workflow events separately from the current-state workflow read:

- `GET /workflows/{id}` returns the current workflow status snapshot view.
- `GET /workflows` returns `WorkflowInfo` list entries with workflow identity, lifecycle timestamps when available, current state, and task counts.
- `GET /workflows/{id}/events` returns timestamped `WorkflowEventRecord` entries for audit and debugging.
