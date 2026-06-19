## 1. Orchestrator Models And Validation

- [x] 1.1 Add `workspace` / `workspace.group_name` fields to orchestrator `TaskDef` models with serde defaults that preserve existing workflow YAML compatibility.
- [x] 1.2 Add workspace group identifier validation during workflow registration, rejecting invalid group names and multiple workspace groups per task.
- [x] 1.3 Define stable workspace identity keys for private task workspaces and shared workspace groups without persisting worker-local paths in workflow instance state.
- [x] 1.4 Add tests for default private workspace selection, `workspace.group_name` override selection, invalid group rejection, and one-workspace-per-task validation.

## 2. Workspace Manager

- [x] 2.1 Add an orchestrator-side `WorkspaceManager` component with configuration for worker-local workspace root, workspace TTL, and cleanup interval.
- [x] 2.2 Implement `WorkspaceManager` creation/resolution for default logical-task workspaces using a stable key derived from workflow instance ID and task ID.
- [x] 2.3 Implement `WorkspaceManager` creation/resolution for workflow-instance workspace groups using a stable key derived from workflow instance ID and normalized workspace group name.
- [x] 2.4 Ensure each workspace records a timestamp marker usable by stale cleanup.
- [x] 2.5 Implement explicit cleanup for RunHelm-owned workspace directories without adding the background TTL monitor yet.
- [x] 2.6 Add unit tests for workspace key derivation, path construction, stable task workspace reuse across attempts, stable group workspace reuse across tasks, and cleanup.

## 3. Executor Payload Contract

- [x] 3.1 Add selected workspace path to orchestrator executor payloads.
- [x] 3.2 Thread selected workspace path through the orchestrator `ExecutorPort` call path when tasks transition to execution.
- [x] 3.3 Update worker `TaskExecutionPayload` TypeScript models to include selected workspace path.
- [x] 3.4 Update fake and Docker executor adapters to accept and expose exactly one selected workspace path.
- [x] 3.5 Add orchestrator tests proving later attempts for the same logical task receive the same selected workspace path.

## 4. Worker Executor Integration

- [x] 4.1 Update Function executor context so task code can discover and use the selected workspace path.
- [x] 4.2 Update Agent executor prompt or execution context to tell the Agent the selected workspace path for file work.
- [x] 4.3 Update Docker deployment to mount the configured workspace root into reused worker containers.
- [x] 4.4 Add dispatch-level coverage proving `workspace.group_name` selects the group workspace path instead of the task's default private workspace path.
- [x] 4.5 Add executor tests for Agent, Function, and Docker workspace exposure/pass-through behavior.

## 5. File Access Scope

- [x] 5.1 Identify file read/write surfaces available to Agent, Function, and Docker-backed execution.
- [x] 5.2 Document which file access surfaces are guidance-only versus enforceable by RunHelm-owned code.
- [x] 5.3 Confirm the current implementation exposes one selected workspace path but does not enforce selected-workspace-only access for arbitrary task code.
- [x] 5.4 Defer strict path containment for file tools to a future sandbox, per-task container, or validated file-tool design.

## 6. Workflow Semantics

- [x] 6.1 Add workflow-engine tests proving shared workspace groups do not create scheduling dependencies; normal JSON data bindings or control dependencies still determine task ordering.

## 7. Documentation

- [x] 7.1 Document the workflow YAML shape for default private workspace and nested `workspace.group_name`.
- [x] 7.2 Document selected workspace executor context for Agent, Function, Docker, and fake executors.
- [x] 7.3 Document local workspace root configuration, timestamp marker layout, and one-workspace-per-task behavior.
- [x] 7.4 Document operational cleanup behavior and TTL cleanup configuration.

## 8. TTL Monitor

- [x] 8.1 Implement the `WorkspaceManager` background TTL monitor after workspace creation and executor payloads are complete.
- [x] 8.2 Make the TTL monitor wake interval and workspace TTL configurable.
- [x] 8.3 Ensure the TTL monitor only removes RunHelm-owned expired workspace directories under the configured workspace root.

## 9. Verification

- [x] 9.1 Run orchestrator Rust tests covering workflow validation, workspace selection, and executor payload behavior.
- [x] 9.2 Run worker TypeScript tests covering executor workspace exposure.
- [x] 9.3 Run OpenSpec validation/status checks for `add-task-scratch-space`.
