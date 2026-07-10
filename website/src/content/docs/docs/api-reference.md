---
title: API Reference
description: HTTP endpoints for registering workflows, starting runs, inspecting state, controlling execution, and worker coordination.
---

RunHelm exposes a public orchestrator API for users and operators, plus a worker API used by RunHelm workers. Local Docker installs expose the public API on `http://localhost:3000`. The worker API listens separately on `http://localhost:3001` by default and is intended for worker runtime integration.

Examples use:

```bash
export RUNHELM_URL=http://localhost:3000
```

## Public endpoints

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/health` | Check API health. |
| `GET` | `/workflow-def` | List registered workflow definition summaries. |
| `POST` | `/workflow-def` | Register a workflow definition. |
| `POST` | `/workflow-def/{def_id}` | Create and queue a workflow instance. |
| `POST` | `/workflow-def/{def_id}/tasks/{task_id}` | Execute one workflow task in isolation. |
| `POST` | `/function-def` | Register a reusable function definition. |
| `DELETE` | `/function-def/{def_id}` | Delete a reusable function definition. |
| `GET` | `/workflows` | List workflow instances. |
| `GET` | `/workflows/{id}` | Get workflow status. |
| `GET` | `/workflows/{id}/events` | Get workflow event history. |
| `POST` | `/workflows/{id}/pause` | Pause one workflow instance. |
| `POST` | `/workflows/{id}/resume` | Resume one paused workflow instance. |
| `POST` | `/workflows/pause` | Pause active workflow instances. |
| `POST` | `/workflows/resume` | Resume paused workflow instances. |
| `GET` | `/workflows/{workflow_instance_id}/tasks` | List materialized task results. |
| `GET` | `/workflows/{workflow_instance_id}/tasks/{task_id}` | Get the latest result for one logical task. |
| `GET` | `/workflows/{workflow_instance_id}/tasks/{task_id}/{generation}` | Get a specific task generation result. |
| `POST` | `/workflows/{workflow_instance_id}/tasks/{task_id}/human-input` | Submit human input for an `InputNeeded` task. |
| `POST` | `/workflows/{workflow_instance_id}/tasks/{task_id}/retry` | Retry a failed task. |
| `GET` | `/workflow-queue` | Inspect queued workflow instance IDs. |
| `DELETE` | `/workflow-queue/{id}` | Remove one queued workflow instance. |
| `DELETE` | `/workflow-queue` | Purge queued workflow instances. |

## Health

```bash
curl -sS "$RUNHELM_URL/health"
```

Response:

```text
OK
```

## Workflow definitions

Register a workflow definition:

```bash
curl -sS -X POST "$RUNHELM_URL/workflow-def" \
  -H 'content-type: application/json' \
  -d '{
    "id": "hello-workflow",
    "description": "Says hello to a named user.",
    "tasks": [
      {
        "id": "hello",
        "kind": {
          "Function": {
            "dependencies": [],
            "code": "export default async function run({ inputs }) { return { response: `hello ${inputs[0].name}` }; }"
          }
        },
        "input_schemas": [],
        "output_schema": {
          "type": "object",
          "required": ["response"],
          "properties": {
            "response": { "type": "string" }
          }
        },
        "required_credentials": []
      }
    ],
    "data_bindings": []
  }'
```

Response:

```json
{
  "status": "created",
  "id": "hello-workflow"
}
```

`description` is optional and defaults to an empty string.

List registered workflow definitions without loading their task definitions or data bindings:

```bash
curl -sS "$RUNHELM_URL/workflow-def"
```

Response:

```json
{
  "workflow_defs": [
    {
      "id": "hello-workflow",
      "description": "Says hello to a named user.",
      "created_at_epoch_ms": 1780000000000,
      "last_invoked_at_epoch_ms": 1780000001200
    },
    {
      "id": "not-yet-run",
      "description": "A workflow that has not been invoked.",
      "created_at_epoch_ms": 1779999999000,
      "last_invoked_at_epoch_ms": null
    }
  ]
}
```

`last_invoked_at_epoch_ms` is the creation time of the most recently created workflow instance for that definition. It is `null` until the definition has been invoked.

You can overwrite a registered definition while it has no workflow instances.
After any instance has been created, regardless of its state, the definition is
immutable and an overwrite returns `409 Conflict`:

```json
{
  "error": "workflow definition hello-workflow already has workflow instances and cannot be overwritten; register a new ID, for example hello-workflow_v2"
}
```

RunHelm does not enforce a versioning scheme. Suffixes such as `_v2` are a
suggested convention for choosing a new definition ID.

## Function definitions

Register a reusable function definition:

```bash
curl -sS -X POST "$RUNHELM_URL/function-def" \
  -H 'content-type: application/json' \
  -d '{
    "id": "format.hello",
    "dependencies": [],
    "code": "export default async function run({ inputs }) { return { response: `hello ${inputs[0].name}` }; }"
  }'
```

Response:

```json
{
  "status": "created",
  "id": "format.hello"
}
```

Delete a reusable function definition:

```bash
curl -i -X DELETE "$RUNHELM_URL/function-def/format.hello"
```

Successful deletion returns `204 No Content`. Missing definitions return `404 Not Found`.

## Start a workflow

Create and queue a workflow instance from a registered workflow definition:

```bash
curl -sS -X POST "$RUNHELM_URL/workflow-def/hello-workflow" \
  -H 'content-type: application/json' \
  -d '{ "name": "Ada" }'
```

The request body is stored as the trigger input. `null` means no initial input.

Response:

```json
{
  "status": "queued",
  "id": "hello-workflow-1780000000000000000",
  "pinned_host_id": "local-dev-host"
}
```

If no eligible worker host is registered, the API returns `503 Service Unavailable`.

## Execute one task

Run a task from a registered workflow definition without creating a workflow instance:

```bash
curl -sS -X POST "$RUNHELM_URL/workflow-def/hello-workflow/tasks/hello" \
  -H 'content-type: application/json' \
  -d '{ "inputs": [{ "name": "Ada" }] }'
```

Success response:

```json
{
  "status": "success",
  "output": {
    "response": "hello Ada"
  }
}
```

Other result statuses are:

```json
{
  "status": "input_needed",
  "description": "Which release channel should I use?"
}
```

```json
{
  "status": "failure",
  "error_message": "Missing required credential: gh_token"
}
```

## List workflows

```bash
curl -sS "$RUNHELM_URL/workflows?status=completed&limit=20"
```

Query parameters:

- `status`: `pending`, `running`, `paused`, `input_needed`, `completed`, or `failed`.
- `limit`: maximum number of workflows to return.
- `cursor`: pagination cursor returned by the previous response.

Response:

```json
{
  "workflows": [
    {
      "id": "hello-workflow-1780000000000000000",
      "workflow_def_id": "hello-workflow",
      "created_at_epoch_ms": 1780000000000,
      "modified_at_epoch_ms": 1780000001200,
      "completed_at_epoch_ms": 1780000001200,
      "status": "Completed",
      "total_task_count": 1,
      "completed_task_count": 1
    }
  ],
  "next_cursor": null
}
```

## Workflow status

```bash
curl -sS "$RUNHELM_URL/workflows/hello-workflow-1780000000000000000"
```

Response:

```json
{
  "instance_id": "hello-workflow-1780000000000000000",
  "workflow_def_id": "hello-workflow",
  "status": "Completed",
  "tasks": [
    {
      "task_attempt_id": "hello[1]",
      "task_def_id": "hello",
      "status": "Completed",
      "satisfaction": "Satisfied",
      "generation_index": 1
    }
  ],
  "verifier_states": []
}
```

## Workflow events

```bash
curl -sS "$RUNHELM_URL/workflows/hello-workflow-1780000000000000000/events"
```

Response shape:

```json
{
  "workflow_instance_id": "hello-workflow-1780000000000000000",
  "events": []
}
```

Events are an execution history for the workflow instance. Exact event payloads depend on the workflow operations that have occurred.

## Pause and resume

Pause one workflow:

```bash
curl -sS -X POST "$RUNHELM_URL/workflows/hello-workflow-1780000000000000000/pause"
```

Response:

```json
{
  "status": "paused",
  "workflow_instance_id": "hello-workflow-1780000000000000000"
}
```

Resume one workflow:

```bash
curl -sS -X POST "$RUNHELM_URL/workflows/hello-workflow-1780000000000000000/resume"
```

Response:

```json
{
  "status": "queued",
  "workflow_instance_id": "hello-workflow-1780000000000000000"
}
```

Pause all active workflows:

```bash
curl -sS -X POST "$RUNHELM_URL/workflows/pause"
```

Resume all paused workflows:

```bash
curl -sS -X POST "$RUNHELM_URL/workflows/resume"
```

Bulk responses include the operation status, affected count, and workflow instance IDs:

```json
{
  "status": "paused",
  "count": 2,
  "workflow_instance_ids": [
    "workflow-a-1780000000000000000",
    "workflow-b-1780000000000000000"
  ]
}
```

## Task results

List task results for a workflow instance:

```bash
curl -sS "$RUNHELM_URL/workflows/hello-workflow-1780000000000000000/tasks"
```

Response:

```json
{
  "workflow_instance_id": "hello-workflow-1780000000000000000",
  "tasks": [
    {
      "task_attempt_id": "hello[1]",
      "result": {
        "status": "success",
        "input": [
          {
            "name": "Ada"
          }
        ],
        "output": {
          "response": "hello Ada"
        },
        "task_def_id": "hello",
        "task_attempt_id": "hello[1]",
        "satisfaction": "Satisfied",
        "generation_index": 1
      }
    }
  ]
}
```

Get the latest result for one logical task:

```bash
curl -sS "$RUNHELM_URL/workflows/hello-workflow-1780000000000000000/tasks/hello"
```

Get a specific generation:

```bash
curl -sS "$RUNHELM_URL/workflows/hello-workflow-1780000000000000000/tasks/hello/1"
```

Task result statuses include `success`, `failure`, `pending`, `running`, and `input_needed`. Success results include `output`; failure results include `error_message`; input-needed results include `input_request`.

## Human input

Submit input for an Agent task currently waiting in `InputNeeded`:

See [Human Input](/docs/concepts/human-input/) for the full workflow behavior and Agent configuration.

```bash
curl -sS -X POST "$RUNHELM_URL/workflows/human-workflow-1780000000000000000/tasks/collect-release-preference/human-input" \
  -H 'content-type: application/json' \
  -d '{ "input": "Use the stable release channel." }'
```

Response:

```json
{
  "status": "queued",
  "workflow_instance_id": "human-workflow-1780000000000000000",
  "task_attempt_id": "collect-release-preference[2]"
}
```

The API returns `409 Conflict` if the task is not waiting for input or is not an Agent task.

## Retry tasks

Retry a failed task on the workflow's existing pinned host:

```bash
curl -sS -X POST "$RUNHELM_URL/workflows/hello-workflow-1780000000000000000/tasks/hello/retry"
```

Response:

```json
{
  "status": "queued",
  "workflow_instance_id": "hello-workflow-1780000000000000000",
  "task_attempt_id": "hello[1]",
  "pinned_host_id": "local-dev-host",
  "local_context_may_be_lost": false
}
```

Force retry can reassign the workflow to another eligible host when the original host is unavailable:

```bash
curl -sS -X POST "$RUNHELM_URL/workflows/hello-workflow-1780000000000000000/tasks/hello/retry?force=true"
```

If no retry host is available, the API returns `503 Service Unavailable`.

## Workflow queue

Inspect pending workflow instance IDs:

```bash
curl -sS "$RUNHELM_URL/workflow-queue"
```

Response:

```json
{
  "pending": [
    "hello-workflow-1780000000000000000"
  ]
}
```

Remove one queued workflow instance:

```bash
curl -i -X DELETE "$RUNHELM_URL/workflow-queue/hello-workflow-1780000000000000000"
```

Successful removal returns `204 No Content`.

Purge the queue:

```bash
curl -sS -X DELETE "$RUNHELM_URL/workflow-queue"
```

Response:

```json
{
  "status": "purged",
  "purged": [
    "hello-workflow-1780000000000000000"
  ]
}
```

## Worker endpoints

Worker endpoints are for RunHelm worker processes. Application clients normally use the public endpoints above.

Use a separate worker API base URL:

```bash
export RUNHELM_WORKER_URL=http://localhost:3001
```

### Register worker

```bash
curl -sS -X POST "$RUNHELM_WORKER_URL/workers/register" \
  -H 'content-type: application/json' \
  -d '{
    "worker_id": "worker-1",
    "host_id": "local-dev-host"
  }'
```

Response:

```json
{
  "type": "registration_ack",
  "worker_id": "worker-1",
  "heartbeat_interval_ms": 5000
}
```

### Heartbeat worker

```bash
curl -sS -X POST "$RUNHELM_WORKER_URL/workers/heartbeat" \
  -H 'content-type: application/json' \
  -d '{
    "worker_id": "worker-1",
    "host_id": "local-dev-host"
  }'
```

Response:

```json
{
  "status": "accepted",
  "worker_id": "worker-1"
}
```

### Claim task

```bash
curl -sS -X POST "$RUNHELM_WORKER_URL/workers/tasks/claim" \
  -H 'content-type: application/json' \
  -d '{ "worker_id": "worker-1" }'
```

Task response:

```json
{
  "type": "task_dispatch",
  "workflow_inst_id": "hello-workflow-1780000000000000000",
  "task_id": "worker-pool-0",
  "task": {
    "id": "hello",
    "kind": {
      "Function": {
        "dependencies": [],
        "code": "export default async function run() { return { response: \"ok\" }; }"
      }
    },
    "required_credentials": []
  },
  "workspace_path_suffix": "hello-workflow-1780000000000000000/taskid-hello",
  "inputs": [
    {
      "name": "Ada"
    }
  ],
  "execution_metadata": {}
}
```

No-task response:

```json
{
  "type": "no_task"
}
```

### Complete task

```bash
curl -sS -X POST "$RUNHELM_WORKER_URL/workers/tasks/worker-pool-0/result" \
  -H 'content-type: application/json' \
  -d '{
    "kind": "success",
    "output": {
      "response": "hello Ada"
    }
  }'
```

Other completion payloads:

```json
{
  "kind": "input_needed",
  "description": "Which release channel should I use?"
}
```

```json
{
  "kind": "failure",
  "reason": "Missing required credential: gh_token"
}
```

Response:

```json
{
  "status": "accepted"
}
```
