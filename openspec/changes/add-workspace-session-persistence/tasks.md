## 1. Domain Models and Storage

- [ ] 1.1 Add core models for stable worker host identity, logical workspace keys, workflow instance host pins, worker heartbeat state, and dispatch leases
- [ ] 1.2 Extend `StoragePort` with workflow pin operations for get-or-create, lookup, update/touch, and fail-on-host-loss semantics
- [ ] 1.3 Extend `StoragePort` with dispatch lease operations for claim, release, expire, and startup recovery queries
- [ ] 1.4 Implement workflow pin and dispatch lease storage in the memory storage adapter
- [ ] 1.5 Add unit tests for workflow pin creation at instance creation, pinned workflow reuse, heartbeat deregistration, pinned host loss failure, force retry reassignment, and lease expiration behavior

## 2. Worker Registration and Affinity-Aware Dispatch

- [ ] 2.1 Require `RUNHELM_WORKER_HOST_ID` during worker startup and include it as `host_id` in worker registration payloads with optional labels and capabilities
- [ ] 2.2 Preserve worker process identity separately from host identity in `WorkerPool`
- [ ] 2.3 Implement heartbeat join-or-renew semantics and deregister workers after the configured missed heartbeat threshold
- [ ] 2.4 Add workflow pin constraints to pending task dispatch state
- [ ] 2.5 Update workflow instance creation to select and persist a `pinned_host_id` from currently registered eligible workers
- [ ] 2.6 Update worker claim logic to dispatch only tasks whose workflow pin matches the claiming worker host
- [ ] 2.7 Prevent more than one active task dispatch per workflow instance
- [ ] 2.8 Record durable dispatch lease metadata when a task is claimed by a worker
- [ ] 2.9 Release or expire dispatch leases on task completion, worker disconnect, timeout, and late result paths
- [ ] 2.10 Add worker pool tests for heartbeat registration, heartbeat deregistration, matching host claims, mismatched host rejection, multiple workers sharing a host, single active task per workflow instance, and pinned host loss behavior

## 3. Workspace and Session Placement

- [ ] 3.1 Refactor workspace key derivation so the orchestrator can dispatch logical workspace metadata without requiring an orchestrator-local path
- [ ] 3.2 Add workflow pin lookup before dispatching tasks for every workflow instance
- [ ] 3.3 Update task dispatch payloads to include logical workspace metadata and the selected workflow pin host
- [ ] 3.4 Move worker-task workspace materialization to the executing worker side using the worker-local workspace root
- [ ] 3.5 Add `.timestamp` touch behavior when the worker creates or resolves a local workspace
- [ ] 3.6 Make host-local reusable Agent sessions respect the workflow pin
- [ ] 3.7 Add tests for shared workspace workflow pin reuse across tasks, paused workflow pin retention, and Agent continuation behavior under a workflow pin

## 4. Workflow Resume and Human Input

- [ ] 4.1 Add workflow recovery logic that discovers non-terminal workflow instances on orchestrator startup
- [ ] 4.2 Reconstruct or reload runnable workflow work while preserving existing workflow host pins
- [ ] 4.3 Add recovery handling for expired in-flight dispatch leases
- [ ] 4.4 Add human-input submission API and domain events for durably recording submitted input
- [ ] 4.5 Materialize or resume human-input continuation attempts with preserved workflow instance ID, logical task ID, and generation lineage
- [ ] 4.6 Mark a pinned workflow instance failed when its pinned host is declared lost after heartbeat policy
- [ ] 4.7 Add default retry behavior that preserves the existing workflow pin
- [ ] 4.8 Add force retry behavior that explicitly reassigns the workflow instance to a new registered host and records that local context may be lost
- [ ] 4.9 Add workflow engine/API tests for restart recovery, terminal workflow non-requeue, human-input resume, pinned host loss failure, default retry on same host, force retry reassignment, and user-visible retry/give-up state

## 5. Cleanup, Documentation, and Compatibility

- [ ] 5.1 Update workspace cleanup so TTL deletion skips workspaces owned by `Pending`, `Running`, or `InputNeeded` workflow instances
- [ ] 5.2 Allow cleanup of expired workspaces only for terminal workflow instances or explicit administrative deletion
- [ ] 5.3 Document required `RUNHELM_WORKER_HOST_ID` configuration, first-claim workflow pinning, and single-host compatibility behavior
- [ ] 5.4 Update `docs/` with remote-worker workflow pinning, heartbeat liveness, Agent session behavior under pins, pause/resume, pinned-host failure, default retry, force retry reassignment, lease recovery, and cleanup behavior
- [ ] 5.5 Document that workers missing `RUNHELM_WORKER_HOST_ID` fail startup or registration instead of falling back to auto-detected identity

## 6. Validation

- [ ] 6.1 Run targeted orchestrator unit tests for storage, worker pool, workspace manager, and workflow engine changes
- [ ] 6.2 Run full orchestrator test suite
- [ ] 6.3 Run `openspec validate add-workspace-session-persistence --strict`
