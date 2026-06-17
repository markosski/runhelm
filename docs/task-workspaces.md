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

As with Function tasks, the prompt provides task guidance. File tool path containment is handled separately.

## Docker Compose Workspace Root

The Docker Compose deployment mounts the shared `runhelm-workspaces` volume at `/workspaces` in both the orchestrator and worker containers. The orchestrator uses `RUNHELM_WORKSPACE_ROOT=/workspaces`, so selected task workspace paths are created under that mounted root before they are sent to workers.

The worker container is reused across tasks, so Docker cannot remount only one selected workspace subdirectory per task dispatch. Runtime writable locations such as `/tmp` and `/home/runhelm/.cache` remain available for executor internals, while task file work is directed to the selected path under `/workspaces`.
