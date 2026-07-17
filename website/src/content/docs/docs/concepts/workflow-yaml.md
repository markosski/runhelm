---
title: Workflow YAML Reference
description: Reference the workflow definition fields used by RunHelm.
---

RunHelm workflow definitions are JSON or YAML documents. The API accepts either
format directly and stores the definition canonically as JSON.

## Top-level fields

```yaml
id: example-workflow
description: Summarize an input document.

tasks: []

data_bindings: []
```

| Field | Required | Description |
| --- | --- | --- |
| `id` | Yes | Workflow definition ID. IDs are normalized during registration. |
| `description` | No | Human-readable workflow description used in workflow discovery lists. Defaults to an empty string. |
| `tasks` | Yes | Task definitions that make up the workflow graph. |
| `data_bindings` | Yes | Edges that pass outputs from source tasks to target task inputs. |

## Task fields

```yaml
tasks:
  - id: summarize
    timeout_secs: 300
    kind:
      Function:
        dependencies: []
        code: |
          export default async function run({ inputs }) {
            return { response: "ok" };
          }
    input_schemas: []
    output_schema:
      type: object
    workspace:
      group_name: repo
    required_credentials: []
```

| Field | Required | Description |
| --- | --- | --- |
| `id` | Yes | Logical task ID used by bindings, retries, and results. |
| `kind` | Yes | One of `Agent`, `Function`, or `ApiCall`. |
| `timeout_secs` | No | Task execution timeout. |
| `input_schemas` | No | JSON Schemas for expected input slots. |
| `output_schema` | No | JSON Schema for task output. Verifier tasks must omit this because RunHelm injects the decision schema. |
| `workspace` | No | Workspace group assignment. |
| `required_credentials` | Yes | Named credentials required before execution. Use `[]` when none are needed. |
| `control` | No | Verifier control settings for bounded loops. |

## Task kinds

Agent task:

```yaml
kind:
  Agent:
    model_id: "google/gemini-2.5-flash"
    provider_url: ""
    prompt: Summarize the input.
    tools: []
    skills: []
    ask: false
    schema_failure_retry_times: 2
    reuse_session: true
```

Function task:

```yaml
kind:
  Function:
    dependencies:
      - name: lodash-es
        version: 4.17.21
    code: |
      export default async function run({ inputs }) {
        return { response: inputs.length };
      }
```

Registered Function reference:

```yaml
kind:
  Function:
    ref: mailgun.fetch_inbound_mail
```

API Call task:

```yaml
kind:
  ApiCall:
    url: "https://example.com/status"
    method: "GET"
```

## Data bindings

Data bindings pass one task's output into another task's input array:

```yaml
data_bindings:
  - source_task_id: fetch-data
    target_task_id: summarize
```

If a task has multiple upstream bindings, it receives multiple input values. The target task should declare `input_schemas` when it depends on specific input shapes.

## Verifier control

```yaml
control:
  verifier:
    max_iterations: 3
    on_exhausted_continue: false
    rerun_from_task_id: draft-report
```

`control.verifier` turns a task into a bounded-loop verifier. The verifier returns `{ "decision": "complete" }` or `{ "decision": "continue", "feedback": "..." }`.

See [Bounded Loops](/docs/concepts/bounded-loops/) for the full control-flow behavior.

## Workspaces

```yaml
workspace:
  group_name: repo
```

Tasks with the same `workspace.group_name` share the same workflow-instance workspace. Workspace sharing does not create scheduling dependencies; use `data_bindings` to order tasks.

## Credentials

```yaml
required_credentials:
  - llm_api_key
  - gh_token
```

Credentials are resolved by the worker before task execution. Missing credentials fail the task before its main work runs.
