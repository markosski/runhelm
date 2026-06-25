# Task Workspaces

RunHelm passes each task execution one selected workspace path. The path is prepared before executor code runs and points to the task's private workspace or to its declared workspace group. When a task declares `workspace.group_name`, that group workspace replaces the task's default private workspace.

## Workflow YAML

Every task receives a private logical-task workspace by default. Existing workflow YAML does not need to opt in:

```yaml
id: report-workflow
tasks:
  - id: draft-report
    kind:
      Agent:
        model_id: openai/gpt-4.1
        provider_url: ""
        prompt: Draft the report.
        tools: []
        skills: []
    output_schema:
      type: object
    required_credentials: []
data_bindings: []
```

Use `workspace.group_name` when multiple tasks should intentionally share one workflow-instance workspace:

```yaml
id: repo-workflow
tasks:
  - id: clone-repo
    kind:
      Function:
        dependencies: []
        code: |
          export default async function run({ workspacePath }) {
            return { workspacePath };
          }
    workspace:
      group_name: repo
    output_schema:
      type: object
    required_credentials: []

  - id: analyze-repo
    kind:
      Agent:
        model_id: openai/gpt-4.1
        provider_url: ""
        prompt: Analyze the repository files in the selected workspace.
        tools: []
        skills: []
    workspace:
      group_name: repo
    output_schema:
      type: object
    required_credentials: []
data_bindings:
  - source_task_id: clone-repo
    target_task_id: analyze-repo
```

See `worker/examples/example_workspace_download_workflow.yaml` for a single Agent task example that downloads a page and saves it in the selected workspace.

Group names use the same conservative identifier style as task IDs. A task receives either its default private workspace or one declared group workspace, never both.

## Function Executor Context

Inline Function tasks receive the selected workspace path in their execution context as `workspacePath`.

```js
export default async function run({ inputs, credentials, workspacePath }) {
  // Write task-local files under workspacePath.
}
```

This field tells Function task code where file work should happen. It does not sandbox arbitrary JavaScript filesystem access; path containment and stronger isolation are handled separately.

## Agent Executor Prompt

Agent tasks receive the selected workspace path in their prompt context. The prompt tells the Agent to use that path for task file work, including continuation attempts that reuse an existing Agent session.

As with Function tasks, the prompt provides task guidance. Strict enforcement that an Agent only reads or writes inside that path is not part of the initial implementation.

## Docker Compose Workspace Root

The Docker Compose deployment mounts the shared `runhelm-workspaces` volume at `/workspaces` in both the orchestrator and worker containers. The orchestrator uses `RUNHELM_WORKSPACE_ROOT=/workspaces`, so selected task workspace paths are created under that mounted root before they are sent to workers.

Docker-backed dispatch passes the selected workspace path through to the worker task payload unchanged.

The worker container is reused across tasks, so Docker cannot remount only one selected workspace subdirectory per task dispatch. Runtime writable locations such as `/tmp` and `/home/runhelm/.cache` remain available for executor internals, while task file work is directed to the selected path under `/workspaces`.

## Fake Executor

The fake executor is used for orchestrator tests and local non-side-effect execution paths. It receives the same selected workspace path through the executor contract, but it does not expose a filesystem API to user task code.

## Workspace Root And Layout

The orchestrator resolves workspace directories under `RUNHELM_WORKSPACE_ROOT` when that environment variable is set. In Docker Compose, this is `/workspaces`, backed by the named `runhelm-workspaces` volume and mounted into both orchestrator and worker containers.

When `RUNHELM_WORKSPACE_ROOT` is not set, local runs use the default workspace root under the user's cache directory. The exact default is a local development/runtime detail; set `RUNHELM_WORKSPACE_ROOT` when deployments need a predictable path.

Workspace paths are deterministic under the configured root:

```text
<workspace-root>/<workflow-instance-id>/taskid-<task-id>
<workspace-root>/<workflow-instance-id>/taskgroup-<group-name>
```

Examples:

```text
/workspaces/inst-1/taskid-draft-report
/workspaces/inst-1/taskgroup-repo
```

Later attempts for the same logical task reuse the same `taskid-<task-id>` path. Tasks that declare the same `workspace.group_name` within one workflow instance reuse the same `taskgroup-<group-name>` path.

Each workspace directory includes a `.timestamp` marker. RunHelm updates that marker when the workspace is created or selected for execution so cleanup can identify stale RunHelm-owned workspaces.

## Workflow Semantics

Workspace groups do not create scheduling dependencies. If two tasks declare the same `workspace.group_name`, they share the same selected workspace path, but the workflow engine still uses normal data bindings and control dependencies to decide when a task is eligible to run.

If a task needs files produced by another task in the same workspace group, the workflow must still declare the normal dependency that orders those tasks.

## Remote Worker Pinning

The OpenSpec change `add-workspace-session-persistence` defines the planned remote-worker behavior for workspace continuity. Workers must be configured with `RUNHELM_WORKER_HOST_ID`; RunHelm does not auto-detect this identity. The value should identify the durable execution state domain that owns the workspace and session roots, not the worker container or process.

Every workflow instance is pinned to a registered worker host when the workflow instance is created for execution. The public workflow trigger path selects a currently eligible registered host and stores it on the workflow instance snapshot. If the workflow definition exists but no eligible worker host is registered, the trigger is rejected as unavailable rather than creating an unpinned queued instance.

After the pin is established, every task in that workflow instance must execute on workers registered with the same host identifier. Multiple worker processes may share the same `RUNHELM_WORKER_HOST_ID` when they share the same durable workspace and session roots; any of those workers can execute work for the pinned workflow.

The in-memory worker registry keeps worker process identity and host identity together as a single worker identity. The worker process ID identifies the live worker that claims or completes a dispatch, while the host ID is the placement identity used for workflow pin matching.

The workflow instance snapshot carries the durable `pinned_host_id`. Worker heartbeat state and active dispatch leases are in-memory `WorkerPool` state for the initial implementation. Dispatch leases track the claimed task attempt, worker process, host, and lease expiration while the orchestrator process is running, and enforce one active dispatch lease per workflow instance. Host-loss failure remains workflow state.

Pending task entries in the worker pool carry dispatch constraints separately from the worker-facing task payload. The current constraint is the workflow instance host pin. This lets the orchestrator preserve placement requirements while work is waiting in memory. When a worker polls for work, the worker pool scans pending tasks for the first task whose constraints match the worker's registered host identity and whose workflow instance does not already have an in-flight task dispatch. Tasks pinned to other hosts, or tasks for a workflow that already has active work, remain pending.

Workers maintain registration by sending heartbeats. A heartbeat with valid worker and host identity joins or renews the worker registration. The orchestrator returns the heartbeat interval during worker registration, and workers use that advertised interval instead of a local heartbeat default. After one missed heartbeat deadline, the orchestrator marks the worker as suspicious and stops assigning new work to it. After the configured missed-heartbeat threshold, the orchestrator deregisters that worker process. A later valid heartbeat may join again.

If the orchestrator restarts before a worker reports a task result, the restarted orchestrator begins with an empty worker registry and no in-memory dispatch leases. For the initial remote-worker implementation, a result from a worker that has not rejoined is rejected as unregistered and does not advance workflow state. Workflow recovery or retry policy handles the corresponding running attempt. A future durable lease-backed result path may accept such results only when the result can be matched to a valid persisted dispatch lease.

RunHelm should dispatch at most one active task at a time for a workflow instance, even when multiple workers share the pinned host. This avoids concurrent writes to the same workflow workspace or host-local Agent session state.

If no worker is currently registered for the pinned host, RunHelm should wait rather than silently moving the workflow to another host. If the host remains unavailable past the host-loss policy, RunHelm should mark the pinned workflow instance as failed. Default retry keeps the same pinned host. A force retry may reassign the workflow to another registered host, but that explicitly accepts that host-local workspace and Agent session context may be lost.

## Cleanup

`WorkspaceManager` removes expired RunHelm-owned workspace directories under the configured workspace root. Cleanup relies on the workspace `.timestamp` marker and only targets the RunHelm workspace layout, not arbitrary paths returned by task code.

The orchestrator starts a background TTL monitor that wakes on `RUNHELM_WORKSPACE_VACUUM_INTERVAL_SECS` and removes workspace directories whose timestamp is older than `RUNHELM_WORKSPACE_TTL_SECS`. The default TTL is 900 seconds, and the default vacuum interval is 60 seconds.

## File Access Scope

The initial workspace implementation provides one selected workspace path and directs task code to use it. It does not sandbox arbitrary Function code, Agent behavior, or reused worker-container processes to only that selected path.

Strict path containment for local file access is deferred to a future design based on RunHelm-owned file tools, per-task containers, or another sandbox that can reject path traversal, absolute-path escapes, and symlink escapes before filesystem access.

Current local file access surfaces:

| Surface | Access behavior | Current workspace control |
| --- | --- | --- |
| Function task code | Inline JavaScript runs in a Node child process and can use normal Node filesystem APIs and installed dependencies. | Guidance-only. The execution context provides `workspacePath`, but arbitrary JavaScript filesystem access is not sandboxed. |
| Function executor runtime files | RunHelm writes temporary `package.json`, `task.mjs`, and `runner.mjs` files under an executor-owned temp directory. | Enforceable by RunHelm-owned code, but this is executor runtime state rather than task artifact workspace state. |
| Agent built-in coding tools | Agent tasks can approve Pi coding tools created with the worker process current directory. These include local file tools such as `read` when approved. | Guidance-only today. The prompt tells the Agent to use the selected workspace path, but the current tool registration is not scoped to that path. |
| Agent extension tools | Pi extension tools can be loaded from configured extension paths or packages and may perform filesystem work according to extension implementation. | Guidance-only unless a future RunHelm-owned tool wrapper validates paths before invoking the extension. |
| Agent skills | Skills are loaded from Pi resource directories and require the `read` tool so the Agent can load `SKILL.md` content. | Guidance-only. Skill loading is separate from selected task workspace access. |
| Agent session store | RunHelm persists Agent conversation sessions under the configured session/cache location. | Enforceable by RunHelm-owned code, but this is worker runtime state rather than task artifact workspace state. |
| Docker Compose worker container | The reused worker container sees the mounted workspace root and receives one selected `workspace_path` per task dispatch. Runtime paths such as `/tmp` and `/home/runhelm/.cache` remain writable for executor internals. | Root-level deployment containment only. The worker container is not remounted per task, so selected-workspace-only access is not enforced. |

The practical contract for this change is: RunHelm selects and exposes exactly one intended task workspace path. It does not claim selected-workspace-only read/write enforcement for arbitrary task code. Future strict containment should be designed around an owned file access boundary, such as validated file tools, per-task containers, or another sandbox.
