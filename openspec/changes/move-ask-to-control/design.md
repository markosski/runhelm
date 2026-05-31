## Context

`TaskTypeDef::Agent` currently owns an `ask: bool` flag, while task-level orchestration controls live under `TaskDef.control`. Verifier support already uses `control.verifier` to influence execution, output schema behavior, rerun slice materialization, task satisfaction, and attempt metadata.

The orchestrator now models workflow execution as materialized task attempts keyed by IDs like `task[1]`, with `generation_index`, `input_mapping`, and verifier state carrying lineage. Human input should fit this same attempt-oriented model: an `InputNeeded` result should pause the workflow, and the supplied answer should create a new attempt of the same logical task instead of mutating the paused attempt in place.

Ask and verifier are similar because both can cause a later task attempt, but they are not the same control. A verifier reviews completed output and may reject it as unsatisfied. Ask means the task did not have enough context to complete, so the previous attempt is incomplete rather than bad. The shared abstraction is attempt creation with explicit cause metadata, not identical lifecycle semantics.

## Goals / Non-Goals

**Goals:**
- Move ask permission from agent-specific task kind data into task control configuration as `control.ask`.
- Keep ask eligibility understandable by allowing `control.ask` only on `Agent` tasks.
- Allow a task with `control.ask` to transition to `InputNeeded` when its executor requests human input.
- Preserve the `InputNeeded` attempt and create a later attempt with the human response attached as execution context or input.
- Bound ask loops with a finite attempt budget and provide prior ask-response history to later attempts.
- Keep downstream tasks blocked until an ask-enabled task has a completed, satisfied attempt.
- Make ask behavior available through the same task-control surface as verifier behavior.

**Non-Goals:**
- Implement a full human interaction UI.
- Redesign verifier loop semantics beyond the interactions needed for ask-created attempts.
- Change ordinary data binding syntax.
- Require every executor kind to support asking for input immediately.

## Decisions

### Use `TaskControl` for ask configuration

Add an ask configuration field to `TaskControl`, for example:

```rust
pub struct TaskControl {
    pub verifier: Option<VerifierControlConfig>,
    pub ask: Option<AskControlConfig>,
}
```

The initial `AskControlConfig` should mirror verifier's finite-control posture by requiring an explicit attempt budget:

```yaml
control:
  ask:
    max_attempts: 3
```

`max_attempts` counts the maximum number of ask-created attempts allowed for the task after the initial attempt asks for input. This keeps workflow pause and retry semantics under orchestration control without encoding durable workflow behavior as an agent-kind field or allowing an unbounded human-input loop.

Alternative considered: keep `kind.Agent.ask` and special-case agents in the engine. That keeps the short-term model smaller, but it duplicates control semantics and makes future non-agent ask support harder.

### Restrict ask to Agent tasks

`control.ask` should be valid only when `TaskDef.kind` is `Agent`. Function and API call tasks do not naturally benefit from asking a human because they execute deterministic or externally delegated work rather than reasoning about whether context is missing.

This creates a two-part mental model:

```text
kind.Agent   = this task can reason interactively
control.ask  = this task is allowed to pause the workflow for human input
validation   = only Agent tasks may declare control.ask
```

Alternative considered: allow `control.ask` on any task kind because the engine can technically process `ExecutionResult::InputNeeded` generically. That makes the schema more uniform, but it is misleading for workflow authors and invites configurations that cannot work usefully.

### Gate `ExecutionResult::InputNeeded` with `control.ask`

The engine should treat `ExecutionResult::InputNeeded` as valid only when the task declares `control.ask`. If a task without ask control returns `InputNeeded`, fail the task definition or execution with a clear error. This prevents executors from unexpectedly pausing workflows.

Alternative considered: allow any executor to return `InputNeeded`. That is simpler operationally, but it removes the workflow author's explicit consent for a human pause.

### Preserve paused attempts and create human-input attempts

When a task attempt returns `InputNeeded`, keep that attempt in `TaskStatus::InputNeeded` and set the workflow to `WorkflowStatus::InputNeeded`. When human input is submitted, create the next attempt for the same logical task, using the next local `generation_index`. The new attempt should record metadata that links it to the paused attempt and carries the human response.

The attempt metadata should distinguish human-input retries from verifier-feedback retries. A later implementation can introduce a general attempt context structure with fields such as cause, previous attempt ID, question, response, verifier task ID, and verifier iteration:

```text
attempt_context:
  cause: initial | human_input | verifier_feedback
  previous_task_attempt_id?: string
  question?: string
  response?: string
  verifier_task_id?: string
  verifier_iteration?: number
```

Ask control also needs task-level state similar in spirit to verifier state. It should track the latest ask attempt, the finite attempt budget, and an ordered history of human responses. Later ask-created attempts should receive that history through execution metadata so the agent can incorporate all prior clarifications, not only the most recent answer.

```text
ask_context:
  attempt: number
  max_attempts: number
  history:
    - question: string
      response: string
```

Alternative considered: update the existing `InputNeeded` attempt and resume it in place. That loses lineage and conflicts with the existing model where each execution is a materialized attempt.

### Treat human responses as execution context, not upstream data binding output

Human input should be supplied to the rerun attempt as explicit attempt context or a synthetic input slot owned by ask handling, not as an output of the paused attempt. The paused attempt did not produce a successful output and must not satisfy downstream bindings.

Alternative considered: store the answer as `output_data` on the paused attempt. That would incorrectly make an incomplete task look completed and would blur the distinction between user answers and task outputs.

### Add a WorkflowService and API unblock path

Human input must enter through an explicit orchestration boundary, not by mutating task state directly. `WorkflowService` should expose a method that accepts a workflow instance ID, waiting task attempt ID, and human response. The service should validate that the target attempt exists, is in `InputNeeded`, belongs to an ask-enabled Agent task, and still has ask budget remaining. It should then append to ask history, materialize the next attempt, and return the new task attempt ID.

The HTTP API should expose this service capability for external callers. The API should preserve WorkflowService as the source of orchestration rules and return clear errors for unknown workflow instances, unknown task attempts, non-waiting attempts, and exhausted ask budgets.

Alternative considered: have the engine poll or infer human input from storage. That would blur service boundaries and make it harder to validate external user actions consistently.

### Keep downstream eligibility based on satisfied completed attempts

`resolve_inputs` and downstream readiness should continue to select completed, satisfied attempts. `InputNeeded` attempts remain unsatisfied and do not propagate. The new human-input attempt becomes eligible to satisfy downstream bindings only after it completes successfully.

The preferred satisfaction state for an `InputNeeded` attempt is pending or otherwise incomplete, not verifier-style unsatisfied. The attempt produced no accepted or rejected output; it merely declared that more information is needed.

Alternative considered: consider `InputNeeded` a special pending state that still satisfies bindings once a response exists. That creates hidden state transitions and makes lineage harder to inspect.

### Replace `kind.Agent.ask` before launch

RunHelm is not operational yet, so this change does not need a compatibility migration path. The implementation should replace `kind.Agent.ask` with `control.ask` before workflows are treated as stable external contracts.

Alternative considered: keep `kind.Agent.ask` as compatibility sugar and normalize it into `control.ask`. That would be appropriate after launch, but it adds unnecessary schema surface while the project is still free to change its workflow definition model.

## Risks / Trade-offs

- `InputNeeded` without a public submit-human-input API can strand workflows -> Add or extend a workflow service method before enabling end-to-end ask retries.
- Verifier rerun slices may contain task-local attempts created by other control causes -> Resolve verifier task inputs from the latest completed source attempt at or after the verifier state's current iteration, and rely on `input_mapping` for exact lineage rather than shared generation numbers.
- Finite ask budgets may be confused with verifier iteration budgets -> Document that ask attempts are created from human responses, while verifier iterations are created from rejected completed outputs.
- Allowing `control.ask` on all task kinds would confuse workflow authors -> Reject ask control on non-Agent tasks during workflow definition validation.
- Removing `kind.Agent.ask` changes the internal workflow schema -> Update tests and docs together so the pre-launch definition model remains consistent.

## Migration Plan

1. Add `AskControlConfig` with `max_attempts` and add `TaskControl.ask`.
2. Update workflow registration validation so `control.ask` is accepted only for `Agent` tasks.
3. Remove `ask` from `TaskTypeDef::Agent` and update workflow definition tests/fixtures accordingly.
4. Update executor result handling so `InputNeeded` is accepted only for ask-enabled tasks.
5. Add ask state/history tracking for human-provided data and expose it in execution metadata for ask-created attempts.
6. Add a WorkflowService method for submitting human input that materializes the next attempt with lineage metadata until `max_attempts` is exhausted.
7. Add an HTTP API route that delegates human input submission to WorkflowService.
8. Update status/result reporting so paused attempts and human-input attempts are visible.
9. Update docs under `docs/` to describe `control.ask` and attempt behavior.

Rollback before launch is straightforward: restore the previous internal model or disable the submit-human-input path if attempt creation proves incomplete.

## Open Questions

- Should the human response be passed to executors through `ExecutionMetadata`, an additional input value, or a dedicated task-attempt context object?
- Should `InputNeeded` attempts use `TaskSatisfactionStatus::Pending` or `Unsatisfied` while waiting for user response?
- How should API callers identify the exact paused attempt when submitting human input if multiple tasks are waiting?
