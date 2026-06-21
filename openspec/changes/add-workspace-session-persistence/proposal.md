## Why

RunHelm currently passes an orchestrator-local workspace path to whichever worker claims a task, which works only when orchestrator and workers share the same host and filesystem. Remote workers, paused workflows, and restarted orchestrators need persisted placement and session state so tasks that share local workspace or Agent session context resume on a compatible host instead of losing state.

## What Changes

- Persist a workflow-instance host pin for every workflow instance when it is created for execution.
- Require workers to configure `RUNHELM_WORKER_HOST_ID` and advertise that stable host identity during registration.
- Treat worker heartbeat messages as join-or-renew liveness signals and remove worker registrations after missed heartbeat thresholds.
- Extend task dispatch semantics so task claims honor workflow pinning.
- Resume paused or restarted workflows from durable workflow state while preserving any workflow host pin and session placement.
- Prevent workspace cleanup from deleting workspaces that may still be needed by pending, running, or input-waiting workflows.
- Mark a pinned workflow instance as failed if its pinned worker host is lost and leave retry/give-up decisions to the user.
- Allow explicit force retry to reassign a failed pinned workflow to a new host while acknowledging local context loss.

## Capabilities

### New Capabilities
- `workflow-resume`: Durable workflow resume, paused workflow continuation, workflow host pin recovery, and pinned-host-loss failure behavior after orchestrator restart or human input.

### Modified Capabilities
- `task-workspace`: Workspace selection changes from an orchestrator-local path assumption to workflow-instance host pinning for all workflow instances, with active-workflow retention.
- `worker-pool-ipc`: Worker registration requires stable host identity from `RUNHELM_WORKER_HOST_ID`, and task dispatch adds workflow pin constraints and affinity-aware task claiming.
- `agent-session-persistence`: Agent session continuation must respect workflow host pinning or report explicit recovery behavior when the pinned host is unavailable.
- `workflow-dataflow-engine`: Human-input continuation and resumed task materialization must preserve logical task identity, generation lineage, and workflow pin constraints.

## Impact

- Affects orchestrator workflow execution, worker registration, worker task claiming, task dispatch payloads, workspace management, and workflow recovery paths.
- Requires storage support for durable workflow host pin metadata and restartable workflow queue or lease state.
- May change worker API payloads by adding host identity and logical workspace metadata while preserving task execution semantics.
- Documentation in `docs/` will need updates describing remote-worker workflow pinning, paused workflow resume, pinned-host failure, and cleanup behavior.
