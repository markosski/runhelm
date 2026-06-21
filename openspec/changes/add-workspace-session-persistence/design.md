## Context

RunHelm currently has three related persistence boundaries:

- Workflow state is moving toward event-backed snapshots through `WorkflowStateManager` and `StoragePort`.
- Workspace paths are derived by `WorkspaceManager` under the orchestrator process' configured local root.
- Worker dispatch currently sends a concrete `workspace_path` to whichever registered worker claims the pending task.

That shape is adequate for a single-machine deployment, but remote workers make the path meaningless unless they share the same filesystem. Paused workflows and orchestrator restarts create the same class of problem: a workflow can resume later, but its workspace or Agent session may exist only on the host that ran the previous task attempt.

The design should treat workspace/session continuity as a workflow-instance scheduling decision, not as an incidental local path. To keep the first distributed-worker implementation simple, RunHelm pins every workflow instance to one host when the workflow instance is created for execution. The orchestrator owns logical workflow state and pinning constraints. Workers own host-local materialization of workspaces and session stores.

## Goals / Non-Goals

**Goals:**

- Persist a stable workflow-instance host pin for every workflow instance.
- Dispatch all tasks for a workflow instance to workers on the pinned host.
- Preserve workflow pins across orchestrator restart and human-input pauses.
- Add enough in-memory worker-pool lease state to prevent duplicate active dispatches while the orchestrator process is running.
- Keep workspace cleanup from deleting state that an active or paused workflow may still need.
- Mark pinned workflow instances failed when the pinned host is lost, so the user can decide whether to give up or retry.
- Keep storage adapters responsible for persistence mechanics, while core code owns domain decisions.

**Non-Goals:**

- Do not introduce distributed filesystem replication in the initial change.
- Do not guarantee failover of host-local workspace contents to a different host.
- Do not expose Agent transcripts or workspace contents through broad workflow status APIs.
- Do not implement full event sourcing, replay-based reads, or event migrations as part of this change.
- Do not require workers to share identical absolute filesystem paths.

## Decisions

### Pin Every Workflow Instance At Creation

Introduce durable workflow-instance placement as a `pinned_host_id` field on the runtime workflow instance model. The workflow instance already has a 1:1 identity relationship with the pin, so storing the selected host on `WorkflowInstance` keeps the placement constraint with the workflow snapshot instead of creating a parallel pin row.

Host-loss failure, retry reason, force-retry decisions, and timing belong in workflow state/events rather than in separate placement metadata, so there is one source of truth for workflow lifecycle.

When a queued workflow instance is created, the orchestrator selects a host ID from the currently registered eligible workers and persists it as `pinned_host_id`. After that, every task in the workflow instance must run on that host. If no eligible worker is registered when a caller tries to create a queued workflow instance, the orchestrator rejects the creation with a capacity-unavailable error rather than creating an unpinned instance.

Rationale: workflow-instance pinning keeps workspaces, host-local Agent sessions, pauses, and restart recovery on one simple placement rule. It avoids separate scheduling logic for shared workspaces and Agent session reuse.

Alternative considered: pin only workflow instances that use workspace groups. That preserves more scheduling flexibility, but Agent session reuse still benefits from locality and the conditional rule adds another branch to recovery behavior. Always pinning trades cross-host parallelism for a simpler and safer first implementation.

### Worker Registration Requires Configured Host Identity

Workers must be configured with `RUNHELM_WORKER_HOST_ID`. Worker startup or registration fails when this value is missing or empty. Registration includes stable host identity, and keeps worker identity separate:

- `worker_id`: identifies one worker process or connection and may change when the process restarts.
- `host_id`: identifies the machine or node whose local workspace/session store contains durable state.

Multiple worker processes may register the same `host_id` when they share the same durable workspace and session roots. All such workers are eligible to execute tasks for workflow instances pinned to that host. The orchestrator treats heartbeat messages as join-or-renew messages: a heartbeat with valid worker identity and `host_id` registers the worker if needed and refreshes its liveness deadline. If a worker misses the configured heartbeat threshold, that worker registration is removed. A later heartbeat with the required identity fields may join again.

Rationale: a worker process can restart without invalidating host-local workspace state. Scheduling needs to match tasks to hosts, not only to current worker process IDs.

Alternative considered: auto-detect `host_id` from hostname, cloud metadata, or Kubernetes metadata. That improves convenience but risks unstable or misleading identity, especially when storage is mounted independently of the container or node name. Requiring explicit configuration forces deployments to choose the identity that matches their durable execution state.

Alternative considered: use `worker_id` as the affinity target. This would make every worker restart look like state loss even when the same host still has the workspace.

### Make Task Claiming Affinity-Aware

Pending work should carry placement constraints derived from the workflow instance pin. A worker can claim a task only when its registration satisfies the task's constraints:

- pin established: only workers with the matching `host_id` may claim
- pinned host unavailable: if at least one worker remains registered for that `host_id`, any matching idle worker may claim
- pinned host lost: after heartbeat loss policy determines no worker for the pinned host is recoverable, the workflow instance is marked failed rather than silently moving to a different host

The queue can stay simple for the first implementation by scanning pending tasks for the first one a worker can claim. Later, this can be optimized with per-host queues. To avoid concurrent writes to the same workflow workspace or Agent session state, the orchestrator dispatches at most one active task per workflow instance at a time, even when multiple workers share the pinned host.

Rationale: claim-time matching preserves the current worker-pull API style while adding deterministic placement.

Alternative considered: orchestrator pushes tasks to a specific worker connection. That can work, but it would be a larger dispatch model change and would still need host-level fallback behavior.

### Workers Materialize Local Workspaces

Task dispatch should carry logical workspace metadata and any workflow pin information. The worker resolves the selected workspace path under its configured workspace root, creates or touches the directory, and passes that local path to task code.

The orchestrator can still derive workspace keys, but it should not create worker-local directories for remote workers.

Rationale: the process that executes task code is the only process guaranteed to have meaningful access to the local workspace filesystem.

Alternative considered: require a shared mounted workspace root across all workers. That may be a valid deployment mode later, but it should be an optional storage strategy rather than a core assumption.

### Keep Worker Registry And Dispatch Leases In Memory

Use in-memory worker-pool state to track current worker registrations, heartbeat deadlines, and active task dispatch leases:

- worker registrations and heartbeat deadlines are liveness state, not durable workflow state
- active dispatch leases record worker ID, host ID, claimed time, and lease expiration for the current orchestrator process
- on orchestrator restart, the worker registry and active leases start empty; workers rejoin by heartbeat, and running workflow tasks are recovered from durable workflow snapshots/events

Workflow snapshots remain the authoritative state for workflow status, task status, outputs, and `pinned_host_id`. Worker registry and dispatch lease records are operational scheduling state.

Rationale: workflow state answers "what has happened"; worker-pool state answers "who currently owns this execution attempt in this process." Persisting liveness and lease rows would add cleanup and reconciliation work without improving correctness for the first implementation, because workers must heartbeat again after an orchestrator restart anyway.

Alternative considered: store durable dispatch leases. That would allow future lease-backed acceptance of late task results after orchestrator restart, but it requires dispatch IDs in result payloads and careful matching of worker, host, workflow instance, task attempt, expiration, and grace-period policy. Defer that until the worker result protocol needs it.

For the first implementation, worker registration is still required before accepting task results. If the orchestrator restarts with an empty in-memory worker registry and receives a completion result from a worker that has not rejoined, the orchestrator rejects the result as unregistered and leaves workflow recovery or retry policy to handle the running attempt. This may rerun work that completed just before the restart, but it avoids advancing workflow state from an orphan result that cannot yet be tied to a current worker registration.

Future option: once task result payloads carry durable dispatch identity and lease matching is fully wired, the orchestrator may persist dispatch leases and accept a result from an unregistered worker only when a durable lease matches the dispatch, worker, host, workflow instance, task attempt, and non-expired or grace-period policy.

### Retry Defaults To Same Host, Force Can Reassign

Retries for failed pinned workflow instances default to the existing `pinned_host_id`. A force retry option may explicitly clear the prior pin and assign the workflow instance to a new registered host. Force retry is allowed to lose host-local workspace and Agent session context; the user chooses it when they decide that loss is acceptable.

Rationale: the safe default preserves context and avoids accidental migration. The force path provides an escape hatch when the original host is gone and the user prefers a best-effort rerun over abandoning the instance.

### Human-Input Resume Reuses Existing Placement

When a task returns `InputNeeded`, workflow state remains durable in `InputNeeded`. A later human-input submission should commit a domain event that records the input and materializes or releases the next attempt according to existing dataflow/session rules.

Any workflow pin created before the pause remains in force. Resume does not select a different host for a pinned workflow instance.

Rationale: pause/resume should preserve continuity. Human input is a workflow event, not a reason to discard local execution state.

Alternative considered: treat resumed attempts as fresh executions with no affinity. That would break shared workspace and Agent session continuity.

### Cleanup Consults Durable Workflow State

Workspace cleanup should not delete state for workflows that are `Pending`, `Running`, or `InputNeeded`. Cleanup can remove host-local workspaces only after the owning workflow is terminal and the retention window has elapsed, or after an explicit administrative deletion.

Rationale: the current TTL marker alone is too weak for paused workflows because human response latency can exceed the TTL.

Alternative considered: increase the default TTL. That reduces but does not remove the risk of deleting paused workflow state.

## Risks / Trade-offs

- Pinned host becomes unavailable -> Worker registrations are removed after missed heartbeats; the workflow instance is marked failed only after host loss policy determines the pinned host has no recoverable workers.
- Pending queue scan may become inefficient with many pinned tasks -> Start with a simple scan, then add host-indexed pending queues if load requires it.
- Workers may register unstable `host_id` values -> Require explicit `RUNHELM_WORKER_HOST_ID` configuration and document that it must identify the durable execution state domain, not the container process.
- Local workspace/session state can still be lost if the host disk is lost -> Treat replication/snapshotting as a future capability, not a hidden guarantee.
- More state must be kept consistent across workflow events and queue leases -> Keep `pinned_host_id` in the workflow snapshot and write tests around claim, crash recovery, host loss, and human-input resume flows.

## Migration Plan

1. Add schema/model types for worker host identity, workflow instance `pinned_host_id`, in-memory worker heartbeat state, and in-memory dispatch leases.
2. Preserve `pinned_host_id` through workflow snapshots and keep worker registration and dispatch lease state inside `WorkerPool`.
3. Extend worker startup, registration, and dispatch payloads to require `RUNHELM_WORKER_HOST_ID`.
4. Move workspace materialization for dispatched worker tasks to the worker side, while preserving local fake/executor behavior for tests.
5. Update workflow instance creation to establish pins from registered worker hosts, and update execution to preserve workflow pins across retries, verifier reruns, and human-input resumes.
6. Update workspace cleanup to consult workflow status and placement ownership before deleting directories.
7. Update `docs/` with remote-worker workflow pinning, pause/resume, and cleanup behavior.

Rollback is simplest before enabling remote workers: keep the current single-host behavior by assigning all workers the same host ID and workspace root. After remote affinity is enabled, rollback requires preserving workflow instance `pinned_host_id` values until affected workflows are terminal.

## Open Questions

- Should force retry create a new workflow instance or clear/reassign the pin on the same workflow instance?
