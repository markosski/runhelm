# Task Workspaces

RunHelm passes each task execution one selected workspace path. The path is prepared before executor code runs and points to the task's private workspace or to its declared workspace group.

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
