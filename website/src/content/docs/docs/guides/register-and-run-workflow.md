---
title: Register and Run a Workflow
description: Register a workflow definition, start a run, inspect status, and read task output.
---

This guide walks through the smallest useful RunHelm API flow:

1. Create a workflow definition.
2. Register a workflow definition.
3. Start a workflow instance.
4. Check workflow status.
5. Read task results.

Set the local API URL:

```bash
export RUNHELM_URL=http://localhost:3000
```

## Create a workflow

The quickest way to create a workflow is to give your coding agent access to the
RunHelm repository. Add RunHelm to your application repository as a Git
submodule so the agent can inspect the current examples and documentation while
it works:

```bash
cd path/to/your-application
git submodule add https://github.com/markosski/runhelm.git runhelm
```

Open your application repository in your coding agent, then adapt this prompt:

```text
Create a RunHelm workflow that [describe the outcome you want].

Use the workflow examples in runhelm/worker/examples/ and the documentation in
runhelm/website/src/content/docs/docs/ as the authoritative references. Inspect
my application to understand the inputs, outputs, APIs, and credentials the
workflow needs.

Keep the workflow as small as possible. Define only the required tasks, data
bindings, input and output schemas, and credentials. Do not invent fields that
are not supported by the current RunHelm examples or documentation.

Save an API-ready JSON workflow definition to [path and filename]. Then explain
the workflow, list the inputs and credentials I must provide, and give me the
curl commands to register and run it against $RUNHELM_URL.
```

Replace the bracketed text with your desired outcome and output path. Review the
generated definition and its credential requirements before registering it.

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

You can register an updated definition under the same ID until its first
workflow instance is created. After an instance exists in any state, including
`Completed` or `Failed`, RunHelm keeps the definition immutable and rejects an
overwrite with `409 Conflict`. Register the update under a new ID instead, for
example `hello-workflow_v2`.

The `_v2` suffix is only a suggested naming convention. RunHelm does not require
or interpret workflow definition versions.

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
