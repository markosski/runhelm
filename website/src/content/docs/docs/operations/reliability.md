---
title: Reliability and Side Effects
description: Design workflow tasks for at-least-once execution, retries, verifier reruns, and worker recovery.
---

RunHelm protects workflow state from stale worker results, but task execution is currently at least once. A task may run more than once after retries, verifier reruns, lease timeouts, worker loss, or orchestrator restart.

Design side-effecting tasks with that behavior in mind.

## At-least-once execution

A logical task attempt can be duplicated when:

- an operator retries a failed task
- a verifier returns `continue` and reruns a slice
- a worker loses its lease or misses heartbeats
- the orchestrator restarts before a worker reports a result
- a late worker result arrives after RunHelm has already recovered and dispatched replacement work

RunHelm ignores late or untracked results for workflow-state advancement, but it cannot undo side effects performed by task code.

## Idempotent tasks

Prefer side effects that are safe to repeat:

- write files to deterministic paths in `workspacePath`
- upsert external records using stable keys
- check whether a resource already exists before creating it
- include workflow instance ID and task attempt ID in external idempotency keys when provider APIs support them

Avoid tasks that blindly create, charge, publish, or notify without a dedupe key.

## Place irreversible work late

Put irreversible side effects after verifiers and human approvals.

For example:

```text
draft -> verify -> ask for approval -> publish
```

Avoid:

```text
publish -> verify
```

If a verifier can reject a slice, every task inside that slice may run again. Keep irreversible work outside rerun slices when possible.

## Workspaces are not durable storage

Workspaces are execution storage, not application storage. A workspace may survive retries on the same pinned host, but workflows should copy durable artifacts to an external system when the result must outlive the worker environment.

See [Task Workspaces](/docs/operations/workspaces/) for workspace behavior.

## Retries and local context

Default retry keeps the workflow pinned to the same worker host so workspace files and Agent sessions remain available.

Force retry may reassign the workflow to another host. Use it when progress matters more than preserving local context, and assume host-local workspace and Agent session state may be lost.
