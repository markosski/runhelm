# Operational Status

RunHelm provides a global view of workflow scheduling and task execution state through the orchestrator status endpoint. This is useful for operators to monitor the system health, identify bottlenecks, and ensure a safe shutdown or restart.

## Endpoint

`GET /orchestrator/status`

This endpoint is available on the public orchestrator API (default port 3000).

## Response Shape

```json
{
  "workflow_queue": {
    "pending_workflow_instance_ids": ["wf-1"],
    "active_workflow_instance_ids": ["wf-2"],
    "pending_count": 1,
    "active_count": 1
  },
  "worker_pool": {
    "registered_worker_count": 2,
    "pending_task_count": 1,
    "in_flight_task_count": 3,
    "in_flight_workflow_instance_ids": ["wf-2", "wf-3"]
  }
}
```

### Fields

#### `workflow_queue`
- `pending_workflow_instance_ids`: IDs of workflow instances waiting in the queue to be processed by the engine.
- `active_workflow_instance_ids`: IDs of workflow instances currently being processed by the engine (an engine pass is active).
- `pending_count`: Total number of pending workflow instances.
- `active_count`: Total number of active workflow instances.

#### `worker_pool`
- `registered_worker_count`: Number of workers currently registered and heartbeating.
- `pending_task_count`: Number of individual tasks waiting for a worker to claim them.
- `in_flight_task_count`: Number of tasks currently being executed by workers.
- `in_flight_workflow_instance_ids`: IDs of workflow instances that have at least one task currently in flight.

## Operational Use Cases

### Monitoring Throughput
If `pending_workflow_instance_ids` or `pending_task_count` is consistently high while `active_count` or `in_flight_task_count` is at its maximum (governed by orchestrator and worker pool capacity), it indicates the system is at capacity and may need more workers or a higher orchestrator concurrency limit.

### Safe Restart / Draining
Before restarting the orchestrator, operators should ideally wait for the system to drain.
1. Pause the workflow queue or stop submitting new workflows.
2. Monitor `GET /orchestrator/status`.
3. Wait until `active_count`, `pending_task_count`, and `in_flight_task_count` all reach zero.
4. It is now safe to restart the orchestrator without interrupting any in-flight work.

Note: RunHelm has built-in recovery for tasks that were in-flight during a crash or restart, but draining is preferred for a clean operational handoff.
