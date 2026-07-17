---
title: Simple Function Workflow
description: A minimal workflow that uses one inline Function task.
---

This example shows a single Function task that reads trigger input and returns structured output.

## Workflow definition

```yaml
id: simple-function-workflow

tasks:
  - id: summarize-user
    kind:
      Function:
        dependencies: []
        code: |
          export default async function run({ inputs }) {
            const user = inputs[0] ?? {};
            const name = user.name ?? "friend";

            return {
              response: `Hello, ${name}!`,
              normalizedName: String(name).trim().toLowerCase()
            };
          }
    input_schemas:
      - type: object
        properties:
          name:
            type: string
    output_schema:
      type: object
      required:
        - response
        - normalizedName
      properties:
        response:
          type: string
        normalizedName:
          type: string
    required_credentials: []

data_bindings: []
```

## Register with the API

The API accepts JSON and YAML. Save the definition above as
`simple-function-workflow.yaml`, then register it directly:

```bash
export RUNHELM_URL=http://localhost:3000

curl -sS -X POST "$RUNHELM_URL/workflow-def" \
  --data-binary @simple-function-workflow.yaml
```

## Start a run

```bash
curl -sS -X POST "$RUNHELM_URL/workflow-def/simple-function-workflow" \
  -H 'content-type: application/json' \
  -d '{ "name": "Ada Lovelace" }'
```

Example response:

```json
{
  "status": "queued",
  "id": "simple-function-workflow-1780000000000000000",
  "pinned_host_id": "local-dev-host"
}
```

## Read the result

```bash
curl -sS "$RUNHELM_URL/workflows/simple-function-workflow-1780000000000000000/tasks/summarize-user"
```

Example response:

```json
{
  "status": "success",
  "input": [
    {
      "name": "Ada Lovelace"
    }
  ],
  "output": {
    "response": "Hello, Ada Lovelace!",
    "normalizedName": "ada lovelace"
  },
  "task_def_id": "summarize-user",
  "task_attempt_id": "summarize-user[1]",
  "satisfaction": "Satisfied",
  "generation_index": 1
}
```

## Why this is useful

This pattern is the smallest RunHelm workflow shape:

- trigger input becomes the Function task input
- task output is validated with `output_schema`
- task result can feed downstream tasks through `data_bindings`
- no credentials or workspace setup are required
