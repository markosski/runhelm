---
title: Tasks
description: Understand the task types that make up RunHelm workflows.
---

Tasks are the executable nodes in a RunHelm workflow. Each task receives structured inputs, runs on a worker runtime, and returns structured output for downstream tasks.

A workflow can combine different task kinds in one run:

- [Agent tasks](/docs/concepts/tasks/agents/) use an AI model, prompt, approved tools, selected skills, and optional human input.
- [Function tasks](/docs/concepts/tasks/functions/) run JavaScript code for deterministic logic, integration glue, parsing, validation, and file work.
- [API call tasks](/docs/concepts/tasks/api-calls/) make direct HTTP-style calls when a workflow step does not need model reasoning or custom code.

## Shared task fields

All task kinds participate in the same orchestration model:

```yaml
tasks:
  - id: summarize
    kind:
      Function:
        dependencies: []
        code: |
          export default async function run({ inputs }) {
            return { response: `received ${inputs.length} input objects` };
          }
    input_schemas: []
    output_schema:
      type: object
      required:
        - response
      properties:
        response:
          type: string
    required_credentials: []
```

Common task fields include:

- `id`: the task identifier used by data bindings, results, and status views.
- `kind`: the task-kind-specific configuration.
- `input_schemas`: optional JSON Schemas for the task inputs.
- `output_schema`: optional JSON Schema for the task output.
- `required_credentials`: named credentials the worker must resolve before execution.
- `timeout_secs`: optional execution timeout.
- `workspace`: optional workspace group for file-based collaboration between tasks.

## Choosing a task kind

Use an Agent task when the step needs language reasoning, tool use, code editing, review, classification, or a decision over ambiguous input.

Use a Function task when the step should be deterministic JavaScript: transform data, call a known SDK, validate an output, write files, or prepare a structured payload.

Use an API call task when a direct request is enough and adding custom code would only make the workflow harder to inspect.
