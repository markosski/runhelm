---
title: Function Registry
description: Register reusable JavaScript functions and reference them from workflow definitions.
---

The Function registry stores reusable Function definitions. A workflow can reference a registered function by `ref` instead of embedding code inline in every task.

Use registered functions for shared integrations, common transformations, or any code that is easier to build and test outside a workflow file.

## Create a function

Give your coding agent access to the current RunHelm function implementations
and documentation. If you have not already done so, add RunHelm to your
application repository as a Git submodule:

```bash
cd path/to/your-application
git submodule add https://github.com/markosski/runhelm.git runhelm
```

Open your application repository in your coding agent, then adapt this prompt:

```text
Create a reusable RunHelm function that [describe what the function should do].

Use the implementations and build tooling in runhelm/functions/ and the
documentation in runhelm/website/src/content/docs/docs/ as the authoritative
references. Ensure to understand the function's inputs, output and credential 
requirements.

Create the function under [path and package name]. Follow the existing RunHelm
source, build, and test patterns. Keep the implementation and its dependencies
as small as possible, pin every runtime dependency to a specific version, and
do not invent fields that are not supported by the current examples or
documentation.

Add tests for the function's behavior and generate the registry-ready JSON and
YAML artifacts. Run the tests and build, then explain the function ID, inputs,
output, dependencies, and credentials that a referencing workflow task must
declare. Give me the curl command to register the JSON artifact against
$RUNHELM_URL.
```

Replace the bracketed text with the desired behavior and output location. Review
the generated code, dependency versions, and credential usage before
registering the function.

## Function definition shape

A registered function definition contains:

```json
{
  "id": "format.hello",
  "dependencies": [],
  "code": "export default async function run({ inputs }) { return { response: `Hello, ${inputs[0].name}!` }; }"
}
```

Fields:

- `id`: registry identifier used by workflow `ref`.
- `dependencies`: npm packages the worker installs before execution.
- `code`: JavaScript ESM source that exports a default function.

## Register a function

```bash
export RUNHELM_URL=http://localhost:3000

curl -sS -X POST "$RUNHELM_URL/function-def" \
  -d '{
    "id": "format.hello",
    "dependencies": [],
    "code": "export default async function run({ inputs }) { const name = inputs[0]?.name ?? \"friend\"; return { response: `Hello, ${name}!` }; }"
  }'
```

Response:

```json
{
  "status": "created",
  "id": "format.hello"
}
```

## Reference a registered function

Use `ref` in the workflow task:

```yaml
tasks:
  - id: hello
    kind:
      Function:
        ref: format.hello
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

The orchestrator resolves the function reference before dispatching the task to a worker. If the reference does not exist, execution fails before task code runs.

## Delete a function

```bash
curl -i -X DELETE "$RUNHELM_URL/function-def/format.hello"
```

Successful deletion returns `204 No Content`.

Delete only when no active or future workflows need the reference. Workflows that still refer to a deleted function cannot execute that Function task.

## Build artifacts from source

The repository includes a function artifact builder under `functions/`. The example package compiles TypeScript into registry-ready JSON:

```bash
cd functions/example
npm install
npm run build
```

The build writes artifacts such as:

```text
functions/example/dist/example.example.json
functions/example/dist/example.example.yaml
```

Register the JSON artifact:

```bash
curl -sS -X POST "$RUNHELM_URL/function-def" \
  --data-binary @functions/example/dist/example.example.json
```

Register a YAML artifact directly without converting it:

```bash
curl -sS -X POST "$RUNHELM_URL/function-def" \
  --data-binary @functions/example/dist/example.yaml
```

## When to use inline functions

Use inline functions for small workflow-local logic:

```yaml
kind:
  Function:
    dependencies: []
    code: |
      export default async function run({ inputs }) {
        return { response: inputs.length };
      }
```

Use registered functions when code is reused, tested separately, or packaged from TypeScript source.
