---
title: Task Workspaces
description: Use workspaces to share files safely between related workflow tasks.
---

Task workspaces let related tasks share files during a workflow run. A task can declare a workspace group so later tasks in the same group can see files created or changed by earlier tasks.

## Workflow YAML

Workspace configuration is declared on tasks:

```yaml
workspace:
  group_name: repo
```

Tasks with the same `group_name` share the same workspace for the workflow run. Data bindings still define ordering; sharing a workspace does not create task dependencies by itself.

## Function executor context

Function tasks receive workspace information in their execution context. This lets functions read or write files in the assigned workspace without needing to know the host layout.

## Agent executor prompt

Agent tasks receive workspace context in their prompt so the agent knows where to inspect and edit files when a workflow step requires file-based work.

## Workspace root and layout

The worker uses a configured workspace root. In Docker-based local installs, the workspace directory is mounted into worker containers so files can persist across task executions for the same workflow workspace group.

Workspaces are scoped to the execution environment and intended to isolate task file access from unrelated host paths.

## Cleanup

Workspace cleanup is a lifecycle concern for the worker environment. Do not treat workspaces as durable application storage unless a workflow explicitly copies artifacts to an external system.
