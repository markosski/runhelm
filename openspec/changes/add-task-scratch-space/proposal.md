## Why

RunHelm tasks are currently best suited to structured JSON inputs and outputs, but many useful workflows need a filesystem boundary for repositories, downloaded datasets, generated reports, binaries, logs, and other artifacts. Adding task workspaces gives executions a controlled place for file-native work while preserving isolation, observability, retry behavior, and explicit sharing.

## What Changes

- Add default private workspace for each logical task within a workflow run so retries, verifier reruns, and continuations for that task can reuse the same files without relying on ambient worker filesystem paths.
- Allow workflows to declare explicit workspace groups with nested task configuration such as `workspace.group_name`, replacing the default private workspace for selected tasks when a pipeline needs repository checkouts, downloaded data, generated artifacts, or intermediate directories to survive task boundaries.
- Expose workspace locations through the task execution contract in a way that workers and file tools can enforce path boundaries.
- Keep workspace sharing opt-in; tasks use an isolated private workspace by default, or one declared shared workspace group when configured.

## Capabilities

### New Capabilities

- `task-workspace`: Defines default per-task workflow-run workspace directories, explicit shared workspace group override behavior, and filesystem isolation semantics.

### Modified Capabilities

- `task-executor`: The task execution payload and executor contract will include workspace context so concrete executors can mount or provide task-local and group-shared filesystem space.
- `workflow-dataflow-engine`: Workflow definitions and validation will account for declared workspace groups while keeping dataflow and task scheduling behavior explicit.

## Impact

- Orchestrator core models for task definitions, task attempts, and execution payloads.
- Workflow registration and validation for workspace group declarations and task membership.
- Orchestrator-side `WorkspaceManager` for creating task workspaces and cleaning expired workspaces with configurable TTL.
- Worker executor adapters for Agent, Function, Docker, and fake execution paths.
- Worker-local filesystem workspace root and directory management for creating, resolving, and cleaning workspace directories.
- Documentation under `docs/` once implementation behavior is defined.
