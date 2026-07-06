---
title: Agent Tasks
description: Use model-backed tasks for reasoning, tool use, and review steps.
---

Agent tasks run through the worker's agent executor. They are useful when a workflow step needs language reasoning, tool use, code editing, review, or a decision over ambiguous input.

```yaml
tasks:
  - id: review-change
    kind:
      Agent:
        model_id: "google/gemini-2.5-flash"
        provider_url: ""
        prompt: |
          Review the implementation against the requested behavior.
          Return a concise result with accepted=true only when the change is complete.
        tools: []
        skills: []
        ask: false
        schema_failure_retry_times: 2
    output_schema:
      type: object
      required:
        - accepted
        - response
      properties:
        accepted:
          type: boolean
        response:
          type: string
    required_credentials:
      - llm_api_key
```

## Prompt and inputs

The agent prompt defines the task objective. Inputs from workflow triggers and upstream data bindings are provided to the task execution context so the agent can reason over earlier outputs.

Use schemas when downstream tasks depend on specific fields. A schema makes the contract explicit and gives RunHelm a clear validation boundary before the workflow continues.

## Tools and skills

`tools` declares the approved tool names the agent may use. `skills` declares selected skills that should be loaded for the task.

Keep tool access narrow. If a task only needs to classify text, it should not receive file or shell tools. If it needs to edit a workspace, pair tool approval with a workspace group and a prompt that names the expected file work.

## Credentials

Agent tasks use the first `required_credentials` entry as the model API key. The full required credential set is also available to approved tools executed by the agent.

## Human input

When `ask` is enabled and the task reaches a point that requires clarification, the workflow can move to `InputNeeded`. After input is supplied, RunHelm can continue the workflow from the persisted state.

See [Human Input](/docs/concepts/human-input/) for the `ask_user` flow, API call, and continuation behavior.

## Verifier loops

Agent tasks can participate in [bounded loops](/docs/concepts/bounded-loops/) as either the task being revised or the verifier task. Use an Agent verifier when the acceptance decision requires judgment, such as reviewing a generated report, assessing a code change, or deciding whether output satisfies ambiguous requirements.

## Sessions

Agent tasks can reuse conversation state across attempts. See [Agent Sessions](/docs/concepts/agent-sessions/) for `reuse_session` behavior and guidance on when verifier Agents should start fresh.
