## Context

RunHelm currently has three related persistence boundaries:

- Workflow state is moving toward event-backed snapshots through `WorkflowStateManager` and `StoragePort`.
- Workspace paths are derived by `WorkspaceManager` under the orchestrator process' configured local root.
- Worker dispatch currently sends a concrete `workspace_path` to whichever registered worker claims the pending task.

That shape is adequate for a single-machine deployment, but remote workers make the path meaningless unless they share the same filesystem. Paused workflows and orchestrator restarts create the same class of problem: a workflow can resume later, but its workspace or Agent session may exist only on the host that ran the previous task attempt.

The design should treat workspace/session continuity as a workflow-instance scheduling decision, not as an incidental local path. To keep the first distributed-worker implementation simple, RunHelm pins every workflow instance to one host on first task execution. The orchestrator owns logical workflow state and pinning constraints. Workers own host-local materialization of workspaces and session stores.

## Goals / Non-Goals

**Goals:**

- Persist a stable workflow-instance host pin for every workflow instance.
- Dispatch all tasks for a workflow instance to workers on the pinned host.
- Preserve workflow pins across orchestrator restart and human-input pauses.
- Add enough durable queue/lease state to recover pending and running work after restart.
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

### Pin Every Workflow Instance On First Claim

Introduce a durable workflow-instance placement record:

- `workflow_instance_id`
- `pinned_host_id`
- `pin_reason`, initially `first_task_claim`
- timestamps for creation and last use

The first eligible worker claim for a workflow instance establishes the `pinned_host_id`. After that, every task in the workflow instance must run on that host.

Rationale: workflow-instance pinning keeps workspaces, host-local Agent sessions, pauses, and restart recovery on one simple placement rule. It avoids separate scheduling logic for shared workspaces and Agent session reuse.

Alternative considered: pin only workflow instances that use workspace groups. That preserves more scheduling flexibility, but Agent session reuse still benefits from locality and the conditional rule adds another branch to recovery behavior. Always pinning trades cross-host parallelism for a simpler and safer first implementation.

### Worker Registration Carries Host Identity

Extend worker registration to include stable host identity, and keep worker identity separate:

- `worker_id`: identifies one worker process or connection and may change when the process restarts.
- `host_id`: identifies the machine or node whose local workspace/session store contains durable state.
- optional labels/capabilities: allow future scheduling filters without changing task semantics.

Rationale: a worker process can restart without invalidating host-local workspace state. Scheduling needs to match tasks to hosts, not only to current worker process IDs.

Alternative considered: use `worker_id` as the affinity target. This would make every worker restart look like state loss even when the same host still has the workspace.

### Make Task Claiming Affinity-Aware

Pending work should carry placement constraints derived from the workflow instance pin. A worker can claim a task only when its registration satisfies the task's constraints:

- workflow not yet pinned: any otherwise eligible worker may claim and establish the workflow's `pinned_host_id`
- pin established: only workers with the matching `host_id` may claim
- pinned host lost: the workflow instance is marked failed rather than silently moving to a different host

The queue can stay simple for the first implementation by scanning pending tasks for the first one a worker can claim. Later, this can be optimized with per-host queues.

Rationale: claim-time matching preserves the current worker-pull API style while adding deterministic placement.

Alternative considered: orchestrator pushes tasks to a specific worker connection. That can work, but it would be a larger dispatch model change and would still need host-level fallback behavior.

### Workers Materialize Local Workspaces

Task dispatch should carry logical workspace metadata and any workflow pin information. The worker resolves the selected workspace path under its configured workspace root, creates or touches the directory, and passes that local path to task code.

The orchestrator can still derive workspace keys, but it should not create worker-local directories for remote workers.

Rationale: the process that executes task code is the only process guaranteed to have meaningful access to the local workspace filesystem.

Alternative considered: require a shared mounted workspace root across all workers. That may be a valid deployment mode later, but it should be an optional storage strategy rather than a core assumption.

### Store Queue Leases Separately From Workflow Snapshots

Use durable task lease or dispatch records to recover in-flight work after orchestrator restart:

- pending dispatches can be reconstructed from workflow snapshots and runnable-task analysis, or persisted as queue rows
- claimed/running dispatches record worker ID, host ID, claimed time, and lease expiration
- startup recovery expires stale leases and requeues or fails work according to policy

Workflow snapshots remain the authoritative state for task status and outputs. Queue/lease records are operational scheduling state.

Rationale: workflow state answers "what has happened"; lease state answers "who currently owns this execution attempt." Mixing them would make recovery harder and would push scheduling mechanics into domain snapshots.

Alternative considered: infer all recovery from `TaskStatus::Running`. That is insufficient to distinguish active work from abandoned work after a crash.

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

- Pinned host becomes unavailable -> The workflow instance is marked failed; the user can decide whether to give up or explicitly retry in a future flow.
- Pending queue scan may become inefficient with many pinned tasks -> Start with a simple scan, then add host-indexed pending queues if load requires it.
- Workers may register unstable `host_id` values -> Document configuration expectations and reject missing host IDs once remote affinity is enabled.
- Local workspace/session state can still be lost if the host disk is lost -> Treat replication/snapshotting as a future capability, not a hidden guarantee.
- More state must be kept consistent across workflow events and queue leases -> Keep workflow pin operations behind storage methods and write tests around claim, crash recovery, host loss, and human-input resume flows.

## Migration Plan

1. Add schema/model types for worker host identity, logical workspace keys, workflow instance host pins, and dispatch leases.
2. Extend storage ports and memory storage with workflow pin and lease operations.
3. Extend worker registration and dispatch payloads in a backward-compatible transition where possible.
4. Move workspace materialization for dispatched worker tasks to the worker side, while preserving local fake/executor behavior for tests.
5. Update workflow execution to establish and preserve workflow pins across retries, verifier reruns, and human-input resumes.
6. Update workspace cleanup to consult workflow status and placement ownership before deleting directories.
7. Update `docs/` with remote-worker workflow pinning, pause/resume, and cleanup behavior.

Rollback is simplest before enabling remote workers: keep the current single-host behavior by assigning all workers the same host ID and workspace root. After remote affinity is enabled, rollback requires preserving workflow pin records until affected workflows are terminal.

## Open Questions

- Should retry of a failed pinned workflow create a new workflow instance, clear the pin on the same instance, or require a future explicit migration command?
- Should first-claim pinning be created during enqueue, claim, or worker acknowledgement after local materialization succeeds?
- What is the minimum compatibility period for workers that do not yet send `host_id`?
