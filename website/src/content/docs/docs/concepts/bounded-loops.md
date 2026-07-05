---
title: Bounded Loops
description: Use verifier-controlled reruns to revise part of a workflow without creating cyclic workflow definitions.
---

Bounded loops let a verifier task reject a generation, provide feedback, and ask RunHelm to rerun a configured upstream slice. The workflow definition stays acyclic; `control.verifier` adds a bounded control edge with an explicit iteration limit.

Use bounded loops when a workflow should improve a result before downstream tasks consume it. Common examples include implementation and review, report drafting and validation, extraction and schema checks, or any workflow where a verifier can decide whether a previous step should try again.

<pre class="mermaid">
flowchart TB
    A["Fetch input"]
    B["Produce draft<br/>Agent or Function"]
    C["Verify draft<br/>Agent or Function verifier"]
    D["Publish / downstream task"]

    A --> B --> C
    C -->|"decision: complete"| D
    C -. "decision: continue<br/>feedback + remaining budget" .-> B
</pre>

## Verifier task contract

Any task kind can be a verifier task. That means the verifier itself can be an [Agent task](/docs/concepts/tasks/agents/), [Function task](/docs/concepts/tasks/functions/), or API Call task.

A verifier returns control output, not corrected business data:

```json
{ "decision": "complete" }
```

or:

```json
{
  "decision": "continue",
  "feedback": "Add source links and tighten the recommendation."
}
```

`continue` requires non-empty `feedback`. A task with `control.verifier` must not declare its own `output_schema`; RunHelm injects the verifier decision schema during workflow registration.

## Workflow YAML

Add `control.verifier` to the task that makes the accept-or-rerun decision:

```yaml
tasks:
  - id: draft-report
    kind:
      Agent:
        model_id: "google/gemini-2.5-flash"
        provider_url: ""
        prompt: Draft a short report from the input data.
        tools: []
        skills: []
        ask: false
    output_schema:
      type: object
      required:
        - response
      properties:
        response:
          type: string
    required_credentials:
      - llm_api_key

  - id: verify-report
    kind:
      Function:
        dependencies: []
        code: |
          export default async function run({ inputs }) {
            const draft = inputs[0]?.response ?? "";
            if (draft.includes("Source:")) {
              return { decision: "complete" };
            }

            return {
              decision: "continue",
              feedback: "Add a Source line before publishing."
            };
          }
    control:
      verifier:
        max_iterations: 3
        on_exhausted_continue: false
        rerun_from_task_id: draft-report
    required_credentials: []

data_bindings:
  - source_task_id: draft-report
    target_task_id: verify-report
```

When `verify-report` returns `continue`, RunHelm records the feedback and creates another generation starting at `draft-report`. When it returns `complete`, downstream tasks consume the accepted generation.

## Agent vs Function verifiers

Use an Agent verifier when acceptance depends on judgment:

- reviewing a code change against issue requirements
- judging whether a written answer is clear and complete
- comparing a generated document to fuzzy acceptance criteria
- deciding whether missing context requires human input

Use a Function verifier when acceptance is deterministic:

- validating JSON fields or numeric thresholds
- checking that files exist in `workspacePath`
- verifying that output includes required source links
- applying business rules that should not vary between runs

You can also mix them. For example, an Agent can produce or review a draft while a Function verifier enforces hard gates before publishing.

## Rerun slice

`rerun_from_task_id` chooses where the next generation starts. In a workflow `A -> B -> C -> D`, if verifier task `D` declares `rerun_from_task_id: B`, a `continue` decision creates new attempts for `B`, `C`, and `D`.

If `rerun_from_task_id` is omitted, only the verifier task reruns. Use that for checks that can become valid after external state changes or human-provided context without rerunning upstream work.

## Exhaustion behavior

`max_iterations` limits how many generations RunHelm will attempt.

When the verifier reaches that limit and still returns `continue`:

- `on_exhausted_continue: false` fails the workflow.
- `on_exhausted_continue: true` continues with the latest schema-valid generation.

Prefer `false` for hard quality gates. Use `true` when imperfect output is still useful and downstream tasks can tolerate it.

## Side effects

Rerun slices may execute more than once. Keep side-effecting tasks idempotent, or place irreversible side effects after the verifier accepts the generation.

For example, generate and verify a pull request body before creating the pull request. Do not create the pull request inside a task that may be rerun by verifier feedback.
