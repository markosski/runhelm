---
title: Function Tasks
description: Write JavaScript functions for deterministic workflow steps.
---

Function tasks run JavaScript ESM code in a Node.js child process managed by the worker. They are a good fit for deterministic workflow steps: parsing, validation, data shaping, API SDK calls, file generation, and integration glue.

## Entry point

A function task must export a default function:

```js
export default async function run({ inputs, credentials, workspacePath }) {
  return {
    response: "function completed",
    inputCount: inputs.length,
    workspacePath
  };
}
```

The return value becomes the task output. Return JSON-compatible values so downstream schemas and data bindings can consume the result predictably.

## Execution context

The function receives one context object:

- `inputs`: an array containing workflow input and upstream task outputs provided through data bindings.
- `credentials`: an object keyed by each name in `required_credentials`.
- `workspacePath`: the selected task workspace path for file work.

Required credentials are also exposed to the child process as uppercased environment variables. For example, `gh_token` is available as `process.env.GH_TOKEN`.

## Inline functions

Inline functions keep small task-local code directly inside the workflow definition:

```yaml
tasks:
  - id: normalize-user
    kind:
      Function:
        dependencies: []
        code: |
          export default async function run({ inputs }) {
            const user = inputs[0];
            return {
              response: "normalized user",
              user: {
                name: String(user.name ?? "").trim(),
                email: String(user.email ?? "").toLowerCase()
              }
            };
          }
    required_credentials: []
```

Use inline functions for compact workflow-specific logic that is easier to understand beside the task definition than in a shared registry.

## Registered functions

Registered functions let workflows reference reusable code:

```yaml
tasks:
  - id: fetch-inbound-mail
    kind:
      Function:
        ref: mailgun.fetch_inbound_mail
    required_credentials:
      - mailgun_api_key
      - mailgun_domain
```

Use registered functions for integrations or helpers that multiple workflows share. A registered function definition contains the same `code` and `dependencies` shape as an inline function, but workflows refer to it by `ref`.

## Dependencies

Function dependencies are npm packages declared by name and version:

```yaml
kind:
  Function:
    dependencies:
      - name: lodash-es
        version: 4.17.21
    code: |
      import { startCase } from "lodash-es";

      export default async function run({ inputs }) {
        return { response: startCase(inputs[0].text) };
      }
```

The worker installs declared dependencies before running the function. Keep dependencies specific and minimal because installation is part of task execution cost.

## Local prototyping

Before adding function code to a workflow, prototype it with the same context shape:

```js
import task from "./task.mjs";

const output = await task({
  inputs: [{ text: "hello from runhelm" }],
  credentials: {},
  workspacePath: "/tmp/runhelm-function-prototype"
});

console.log(JSON.stringify(output, null, 2));
```

This catches entry point, import, and output-shape issues before the code is embedded in workflow YAML or registered for reuse.

## Verifier loops

Function tasks can participate in [bounded loops](/docs/concepts/bounded-loops/) as either the task being revised or the verifier task. Use a Function verifier for deterministic checks such as validating fields, applying numeric thresholds, checking files in `workspacePath`, or enforcing business rules that should produce the same decision for the same inputs.

A Function verifier returns the verifier decision directly:

```js
export default async function run({ inputs }) {
  const result = inputs[0];

  if (result.score >= 0.8) {
    return { decision: "complete" };
  }

  return {
    decision: "continue",
    feedback: "Raise the score above 0.8 before continuing."
  };
}
```
