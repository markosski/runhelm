## Context

RunHelm tasks can pass structured JSON through workflow data bindings, but file-native work currently has no first-class execution boundary. Tasks that clone repositories, download datasets, render screenshots, produce reports, need a way to exchange files across steps.

The feature spans orchestrator workflow definitions, executor payloads, worker filesystem setup, and cleanup. The design should keep workspace access explicit and bounded while preserving the current dataflow model: JSON outputs remain the source of scheduling and binding truth, while workspace files are an execution artifact channel.

## Definitions

- Workspace is the user-facing term for this feature: a local filesystem area for files shared across task attempts or explicitly related tasks.

## Goals / Non-Goals

**Goals:**

- Provide every logical task within a workflow run with an isolated private workspace that is reused by that task's attempts.
- Allow workflow authors to explicitly declare a shared workspace group for selected tasks, replacing that task's default private workspace.
- Expose one worker-local workspace directory to each task executor with path containment so each task can read and write only that selected workspace.
- Use worker-local filesystem directories for workspace in the initial implementation.
- Keep task scheduling and structured task inputs/outputs based on JSON data bindings; workspace files provide filesystem access only and do not create implicit dependencies.

**Non-Goals:**

- Making workspace files part of automatic dataflow dependency resolution.
- Automatically sharing all task files within a workflow.
- Exposing multiple workspace directories to a single task in the initial implementation.
- Providing a full artifact browser, file download API, or long-term artifact store.
- Defining distributed shared filesystems, blob-backed workspace storage, or cross-worker remote mount behavior.
- Replacing existing Agent session persistence; workspace is for files, not conversation continuity.

## Decisions

### Use private logical-task workspace by default

Each logical task within a workflow run should receive a private workspace allocation. The allocation is scoped to the workflow instance and logical task id, not to an individual generation or attempt. Retries, verifier-created generations, and human-input continuations for the same task should reuse that task's workspace allocation so task-local files can support iterative work.

This makes task execution easier to reason about for agentic and verifier-driven workflows: task-local files persist across that task's attempts, while downstream tasks still cannot see those files unless they are configured to use the same workspace group. Structured outputs remain the source of scheduling and binding truth.

Private task workspace should be created by default. Workflow authors should not need to opt each task into a basic task-level workspace. If a task declares `workspace.group_name`, that group workspace replaces the default private workspace exposed to the task.

Alternatives considered:

- Attempt-scoped private workspace: provides maximum retry isolation, but loses useful local state for verifier reruns, human-input continuations, and agents incrementally editing repositories or generated files.
- Workflow-wide workspace by default: simpler for file exchange, but creates accidental coupling and weaker isolation.

### Model sharing as declared workspace groups

Workflow definitions should support named workspace groups with explicit task membership through a nested task field such as `workspace.group_name: "foobar"`. A task either uses its default private workspace or the declared shared group workspace. It should not receive both, and it should not receive multiple workspace groups in the initial implementation. Group names should be validated during workflow registration and should use the same conservative identifier style as workflow and task ids.

Workspace group membership should not create scheduling dependencies. If task B needs files produced by task A, the workflow still needs normal dataflow or control dependencies to ensure B runs after A. The group only defines filesystem visibility when B executes.

At runtime, workspace identity should be deterministic rather than stored as local paths on the workflow instance. A private workspace key is derived from workflow instance id and logical task id. A group workspace key is derived from workflow instance id and normalized workspace group name. `WorkspaceManager` owns conversion from those stable keys to local filesystem paths under the configured workspace root, similar to how Agent session storage derives worker-local files from logical session keys.

This lets every task in the same workflow instance and group resolve the same shared location across task runs without persisting worker-local paths in orchestrator workflow state. Physical directory names may include encoded workspace identity and creation timestamp metadata for cleanup, but that layout remains a `WorkspaceManager` implementation detail.

Alternatives considered:

- Infer sharing from data bindings: reduces configuration but conflates structured dataflow with file visibility.
- Let tasks dynamically create shared groups at runtime: flexible but harder to validate and observe.

### Pass workspace context through the executor payload

The orchestrator should include workspace context in the task execution payload sent through `ExecutorPort`. The payload should describe:

- selected workspace name and execution path
- whether the selected workspace is private or a declared group
- task identity needed to select the correct logical-task workspace, such as workflow instance id and task id

Executors should receive local filesystem paths that are already prepared by the worker. Concrete executors can mount those paths into containers, expose them to function code, or include them in Agent execution context.

Agent executors should include the selected task workspace path in the Agent's system or execution prompt so the Agent knows where file work should happen.

Alternatives considered:

- Let each executor independently derive filesystem paths: spreads policy across adapters and makes cleanup inconsistent.
- Return workspace paths from executors after execution: too late for task code to use the workspace boundary consistently.

### Manage workspace under a worker-local filesystem root

The orchestrator side should expose a `WorkspaceManager` component responsible for workspace lifecycle decisions. It should derive stable workspace keys from workflow/task/group identity, create or resolve selected workspace directories for those keys, and clean up RunHelm-owned workspace directories.

Workers should create workspace directories under a configured local root, using a RunHelm-owned layout derived from workflow instance id, logical task id, workspace group name, and creation timestamp. This should remain ordinary filesystem management rather than a generalized storage abstraction.

The directory manager should enforce path containment by construction. Task code may receive a root directory, but worker cleanup should operate on RunHelm-owned directories rather than trusting arbitrary paths returned by task code.

Including a timestamp in workspace directory names gives later cleanup processes a simple staleness signal when a worker, workflow, or task fails before normal cleanup runs.

Workspace cleanup should support a configurable TTL. `WorkspaceManager` should include a monitor that runs on a background thread, wakes on a configured interval, and attempts to clean expired workspaces. This monitor is operationally useful but should be implemented at the end of the change after workspace creation, executor payloads, and path validation are working.

Path containment should also be enforced at tool-call boundaries where possible. File read and write tools used by Agent or Function execution should reject paths that resolve outside the task's selected workspace path, including `..` traversal, absolute-path escapes, and symlink escapes.

Alternatives considered:

- Directly create ad hoc directories inside executor adapters: quicker initially, but spreads path policy across adapters and makes cleanup inconsistent.
- Store all workspace contents in the orchestrator database: inappropriate for large directories, binary files, and repositories.
- Rely only on external cleanup scripts: keeps core implementation smaller, but misses the configured TTL lifecycle RunHelm needs to own for predictable local disk usage.

## Risks / Trade-offs

- Shared workspace can hide ordering bugs -> Require normal dataflow or control dependencies for execution order; workspace group membership alone never schedules tasks.
- Worker disk exhaustion -> Enforce configurable workspace roots and basic cleanup for RunHelm-owned allocations.
- Path traversal or task escape -> Allocate roots through RunHelm-owned local directory management and validate file tool calls against allowed workspace prefixes after path resolution.
- Sensitive files retained after failure -> Keep the first implementation's cleanup behavior simple and document that long-term retention is out of scope.
- Cross-worker execution with local shared workspace -> Treat shared workspace groups as worker-local in the first implementation; workflows that require shared workspace must run those tasks where the same local workspace root is accessible.
- Retry contamination -> Keep default private workspace scoped to one logical task and make retry reuse explicit in the execution contract; require a workspace group override for intentional persistence across task boundaries.
- Docker and non-Docker executors diverge -> Put workspace setup in a shared worker boundary and keep executor-specific code limited to mounting or passing prepared paths.

## Migration Plan

1. Add the nested task-level `workspace.group_name` workflow definition field for optional workspace group membership, with registration-time validation.
2. Add orchestrator models for the selected logical-task or group workspace path that materialized attempts reference in executor payloads.
3. Define stable workspace identity keys for private task workspaces and shared workspace groups without persisting worker-local paths in workflow instance state.
4. Add `WorkspaceManager` creation and cleanup operations backed by worker-local filesystem directory management, including deterministic path derivation and timestamped directory names.
5. Thread workspace context through Agent, Function, Docker, and fake executors.
6. Add file tool path validation against allowed workspace directories where those tools are available.
7. Add basic explicit cleanup for RunHelm-owned workspace allocations.
8. Add the `WorkspaceManager` TTL monitor as the final implementation step, with configurable cleanup interval and TTL.
9. Update `docs/` with the workflow YAML shape, single-workspace executor context, Agent workspace prompt behavior, TTL cleanup configuration, and operational cleanup behavior.

Rollback can disable workspace group declarations and ignore workspace context for workflows that do not use the feature. Existing workflows without workspace configuration should continue to execute with default private workspace behavior or no-op workspace setup, depending on the rollout flag.
