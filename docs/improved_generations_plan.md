# Spec Document: Unified Task Attempts For Human Input And Verifier Feedback

## Summary

Create a new spec-style document at `docs/task-attempts-and-feedback-generations.md` that captures the proposed design shift:

- Treat every task execution as a materialized task attempt.
- Keep task attempt numbering local to each task.
- Separate verifier loop iteration from task generation numbers.
- Model both verifier feedback and human `ask` responses as causes for creating a new task attempt.
- Use `input_mapping` / attempt lineage as the source of truth instead of assuming same-generation propagation across a verifier slice.

## Key Content

- Explain the limitation of the current model:
  - `generation_index` currently acts as both task attempt number and verifier loop generation.
  - Verifier slice bindings currently assume same-generation inputs, e.g. `V[2]` consumes `B[2]`.
  - This makes human-input retries inside verifier slices awkward.

- Define the preferred model:
  - `TaskInstance` represents one attempt of one logical task.
  - `generation_index` is local per task, e.g. `B[2]` does not imply `V[2]`.
  - `VerifierGenerationState` or equivalent stores verifier iteration separately.
  - `input_mapping` records the exact source attempts consumed.

- Include examples:
  - Human input retry:
    ```text
    B[1] -> InputNeeded
    B[2] -> completed with human response
    ```
  - Verifier retry after human input:
    ```text
    A[1] -> B[2] -> V[1]
    verifier rejects from A
    A[2] -> B[3] -> V[2]
    ```
  - Show why this is more orthogonal than requiring `A[1] -> B[1] -> V[1]`.

- Clarify semantics:
  - Human `ask` does not mean the previous attempt was bad; it means incomplete.
  - Verifier `continue` means reviewed work was unsatisfied.
  - Both create new attempts, but with different cause metadata.

## Interface Notes

Current implementation note:

- `TaskInstance` stores the logical task id in `task_def_id`.
- `WorkflowInstance.tasks` stores attempts keyed by `task_attempt_id`, e.g. `task-a[2]`.
- Attempt metadata exposes scalar `generation_index`; `task_attempt_id` is derived from the task map key and the logical task id is represented by `task_def_id`.

The document should propose, not implement, metadata such as:

```text
attempt_context:
  cause: human_input | verifier_feedback
  previous_task_attempt_id: string
  question?: string
  response?: string
  feedback?: string
  verifier_task_id?: string
  verifier_iteration?: number
```

It should also call out that this is a future design direction and differs from the current bounded-loop implementation.

## Test Scenarios To Capture Later

- Human input on a normal task creates `task[2]` and preserves `task[1]` as `InputNeeded`.
- Human input inside a verifier rerun slice does not force all tasks to share the same generation number.
- Verifier rejection uses exact consumed attempts from `input_mapping`.
- Downstream tasks consume only satisfied attempts.
- Status APIs expose attempt cause metadata and source attempt mappings clearly.

## Assumptions

- This is a design/spec note, not an implementation change.
- The document belongs under `docs/` so it follows the project docs convention.
- Current implementation remains documented separately in `docs/bounded-workflow-loops.html`.
