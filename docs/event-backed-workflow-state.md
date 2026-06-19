# Event-Backed Workflow State

RunHelm is considering an event-backed snapshot model for workflow instance state changes. The proposal is tracked in `openspec/changes/add-event-backed-workflow-state/`.

The intended middle ground is:

- workflow state changes are represented as ordered domain events
- core reducer logic applies events to produce the next `WorkflowInstance` snapshot
- a core workflow instance state store coordinates loading the current snapshot, applying the reducer, and asking storage to persist the raw events plus updated snapshot
- storage adapters persist raw events, snapshots, and lightweight summary rows, but do not interpret event meaning
- current workflow reads continue to use snapshots rather than replaying events
- workflow instance lists use one filtered summary query instead of returning full workflow instances

This is not full event sourcing. Full replay-based reads, snapshot checkpoints, optimistic concurrency, event version migrations, and durable database transaction details remain future work until RunHelm needs durable storage semantics.

The main boundary is that event processing belongs in core/domain code. Storage should provide persistence mechanics only, so memory, SQLite, or Postgres adapters can remain swappable without duplicating workflow transition rules. Storage may update `WorkflowInfo` summary rows when it saves a reduced `WorkflowInstance`, but those summaries are derived from the snapshot, not from storage interpreting event payloads.

The intended storage read shape is also narrower than the current snapshot API. `get_workflow_instance` remains the only storage operation that returns full workflow state. List calls should return lightweight summaries with identity, lifecycle timestamps when available, current state, the dynamic workflow flag, and task completion counts.
