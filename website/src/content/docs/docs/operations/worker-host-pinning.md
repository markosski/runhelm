---
title: Worker Host Pinning
description: Understand how RunHelm keeps workflow work on the host that owns its workspace and Agent session state.
---

RunHelm pins each workflow instance to a worker host when the workflow is created. The host identity represents the durable execution state domain that owns workspace and Agent session roots.

Workers identify that host with `RUNHELM_WORKER_HOST_ID`.

## Why pinning exists

Workflows often rely on worker-local state:

- task workspaces
- shared workspace groups
- Agent session files
- in-flight execution context

Pinning keeps every task in a workflow instance on workers registered for the same host identity, so later tasks can see the same workspace and reusable Agent sessions.

## Worker process vs host identity

A worker process has a live worker ID. A host ID identifies the durable execution state domain.

Multiple worker processes may share one `RUNHELM_WORKER_HOST_ID` when they share the same workspace and session roots. Any of those workers can execute work for a workflow pinned to that host.

## Starting workflows

When a workflow instance is created, RunHelm selects an eligible registered host and stores it as `pinned_host_id`.

If no eligible host is registered, workflow start returns `503 Service Unavailable` instead of creating an unpinned queued instance.

## Heartbeats and host loss

Workers maintain registration with heartbeats. After a missed heartbeat, RunHelm stops assigning new work to that worker process. After the missed-heartbeat threshold, RunHelm deregisters the worker process.

If another worker remains registered for the same host ID, future work can continue on that host.

If no worker remains registered for the pinned host, RunHelm waits rather than silently moving the workflow. If the host is considered lost, non-terminal workflows pinned to that host can fail. The workflow pin remains on the failed snapshot.

## Retry behavior

Default retry keeps the existing pinned host:

```bash
curl -sS -X POST "$RUNHELM_URL/workflows/{workflow_instance_id}/tasks/{task_id}/retry"
```

Force retry may reassign the workflow when the pinned host is unavailable:

```bash
curl -sS -X POST "$RUNHELM_URL/workflows/{workflow_instance_id}/tasks/{task_id}/retry?force=true"
```

Force retry records that local context may be lost. Use it only when losing host-local workspace files or reusable Agent session context is acceptable.
