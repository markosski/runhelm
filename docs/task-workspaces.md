# Task Workspaces

RunHelm passes each task execution one selected workspace path. The path is prepared before executor code runs and points to the task's private workspace or to its declared workspace group. When a task declares `workspace.group_name`, that group workspace replaces the task's default private workspace.

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
