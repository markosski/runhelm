## 1. Orchestrator Models And Validation

- [x] 1.1 Add `workspace` / `workspace.group_name` fields to orchestrator `TaskDef` models with serde defaults that preserve existing workflow YAML compatibility.
- [x] 1.2 Add workspace group identifier validation during workflow registration, rejecting invalid group names and multiple workspace groups per task.
- [x] 1.3 Define stable workspace identity keys for private task workspaces and shared workspace groups without persisting worker-local paths in workflow instance state.
- [x] 1.4 Add tests for default private workspace selection, `workspace.group_name` override selection, invalid group rejection, and one-workspace-per-task validation.

## 2. Workspace Manager

- [ ] 2.1 Add an orchestrator-side `WorkspaceManager` component with configuration for worker-local workspace root, workspace TTL, and cleanup interval.
- [ ] 2.2 Implement `WorkspaceManager` creation/resolution for default logical-task workspaces using a stable key derived from workflow instance ID and task ID.
- [ ] 2.3 Implement `WorkspaceManager` creation/resolution for workflow-instance workspace groups using a stable key derived from workflow instance ID and normalized workspace group name.
- [ ] 2.4 Ensure workspace physical directory names include a creation timestamp for stale cleanup.
- [ ] 2.5 Implement explicit cleanup for RunHelm-owned workspace directories without adding the background TTL monitor yet.
- [ ] 2.6 Add unit tests for workspace key derivation, path construction, stable task workspace reuse across attempts, stable group workspace reuse across tasks, and explicit cleanup.

## 3. Executor Payload Contract

- [ ] 3.1 Add selected workspace context to orchestrator executor payloads, including workspace path, workspace kind, and optional group name.
- [ ] 3.2 Thread selected workspace context through the orchestrator `ExecutorPort` call path when tasks transition to execution.
- [ ] 3.3 Update worker `TaskExecutionPayload` TypeScript models to include selected workspace context.
- [ ] 3.4 Update fake and Docker executor adapters to accept and expose exactly one selected workspace path.
- [ ] 3.5 Add orchestrator and worker tests proving later attempts for the same logical task receive the same selected workspace path.

## 4. Worker Executor Integration

- [ ] 4.1 Update Function executor context so task code can discover and use the selected workspace path.
- [ ] 4.2 Update Agent executor prompt or execution context to tell the Agent the selected workspace path for file work.
- [ ] 4.3 Update Docker execution to mount only the selected workspace path into the container.
- [ ] 4.4 Ensure tasks with `workspace.group_name` do not also receive the default private workspace path.
- [ ] 4.5 Add worker tests for Agent, Function, Docker, and fake executor workspace exposure behavior.

## 5. File Tool Path Containment

- [ ] 5.1 Identify file read/write tool surfaces available to Agent or Function execution.
- [ ] 5.2 Add path resolution validation that rejects absolute path escapes, `..` traversal, and symlink escapes outside the selected workspace.
- [ ] 5.3 Apply workspace path validation to all file tools that can access local files.
- [ ] 5.4 Add tests for allowed in-workspace access and rejected traversal, absolute-path, and symlink escape attempts.

## 6. Workflow Semantics

- [ ] 6.1 Verify workspace group membership does not add scheduling edges or alter data-binding readiness.
- [ ] 6.2 Add tests where two tasks share `workspace.group_name` but still require normal JSON data bindings or control dependencies for ordering.
- [ ] 6.3 Add tests confirming workspace file writes are not treated as structured task outputs or downstream inputs.

## 7. Documentation

- [ ] 7.1 Document the workflow YAML shape for default private workspace and nested `workspace.group_name`.
- [ ] 7.2 Document selected workspace executor context for Agent, Function, Docker, and fake executors.
- [ ] 7.3 Document local workspace root configuration, timestamped directory layout, and one-workspace-per-task behavior.
- [ ] 7.4 Document operational cleanup behavior and TTL cleanup configuration.

## 8. TTL Monitor

- [ ] 8.1 Implement the `WorkspaceManager` background TTL monitor after workspace creation, executor payloads, and path validation are complete.
- [ ] 8.2 Make the TTL monitor wake interval and workspace TTL configurable.
- [ ] 8.3 Ensure the TTL monitor only removes RunHelm-owned expired workspace directories under the configured workspace root.
- [ ] 8.4 Add tests for expired workspace cleanup, non-expired workspace preservation, and cleanup staying inside the configured root.

## 9. Verification

- [ ] 9.1 Run orchestrator Rust tests covering workflow validation, workspace selection, and executor payload behavior.
- [ ] 9.2 Run worker TypeScript tests covering executor workspace exposure and file tool containment.
- [ ] 9.3 Run OpenSpec validation/status checks for `add-task-scratch-space`.
