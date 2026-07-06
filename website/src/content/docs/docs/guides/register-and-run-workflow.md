---
title: Register and Run a Workflow
description: Register a workflow definition, start a run, inspect status, and read task output.
---

This guide walks through the smallest useful RunHelm API flow:

1. Register a workflow definition.
2. Start a workflow instance.
3. Check workflow status.
4. Read task results.

Set the local API URL:

```bash
export RUNHELM_URL=http://localhost:3000
```

## Register a workflow

Register a one-task Function workflow:

```bash
curl -sS -X POST "$RUNHELM_URL/workflow-def" \
  -H 'content-type: application/json' \
  -d '{
    "id": "hello-workflow",
    "tasks": [
      {
        "id": "hello",
        "kind": {
          "Function": {
            "dependencies": [],
            "code": "export default async function run({ inputs }) { const name = inputs[0]?.name ?? \"friend\"; return { response: `Hello, ${name}!` }; }"
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

## Start a run

Start a workflow instance from the registered definition:

```bash
curl -sS -X POST "$RUNHELM_URL/workflow-def/hello-workflow" \
  -H 'content-type: application/json' \
  -d '{ "name": "Ada" }'
```

Response:

```json
{
  "status": "queued",
  "id": "hello-workflow-1780000000000000000",
  "pinned_host_id": "local-dev-host"
}
```

Save the returned `id`; it is the workflow instance ID used for status and result reads.

If the API returns `503 Service Unavailable`, no eligible worker host is registered yet.

## Check status

```bash
curl -sS "$RUNHELM_URL/workflows/hello-workflow-1780000000000000000"
```

Example completed response:

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

## Read task results

List all materialized task results:

```bash
curl -sS "$RUNHELM_URL/workflows/hello-workflow-1780000000000000000/tasks"
```

Read one logical task result:

```bash
curl -sS "$RUNHELM_URL/workflows/hello-workflow-1780000000000000000/tasks/hello"
```

Example response:

```json
{
  "status": "success",
  "input": [
    {
      "name": "Ada"
    }
  ],
  "output": {
    "response": "Hello, Ada!"
  },
  "task_def_id": "hello",
  "task_attempt_id": "hello[1]",
  "satisfaction": "Satisfied",
  "generation_index": 1
}
```

## Next steps

Use the [Workflow YAML Reference](/docs/concepts/workflow-yaml/) when building larger definitions, and the [API Reference](/docs/api-reference/) for the full endpoint list.
