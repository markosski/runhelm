## Context

RunHelm currently models a workflow definition as task definitions plus data bindings, and a workflow instance as a map from task definition ID to one `TaskInstance`. A task is considered complete once execution succeeds and output schema validation passes, so downstream bindings can consume that output immediately.

Some workflows need bounded revision across more than one task. For example, a workflow may run `A -> B -> C -> D`, where `D` reviews the combined result and may decide that the workflow should go back to `B`. In that case RunHelm should re-execute `B -> C -> D` as a new bounded iteration while preserving `A`.

The bounded-loop control point should be explicit in workflow definition, but it does not need a separate nested `Loop` body. A verifier block on an Agent task can define a bounded backedge to a previous task in the dependency chain.

## Goals / Non-Goals

**Goals:**

- Add an Agent verifier block that can gate progress after an Agent task runs.
- Let a verifier rerun an arbitrary previous task in the current upstream chain, causing that task and the dependent path back to the verifier to execute again.
- Persist materialized task attempts such as `b[1]`, `c[1]`, `d[1]`, then `b[2]`, `c[2]`, `d[2]`.
- Preserve ordinary acyclic workflow behavior when verifier blocks are absent.
- Keep verifier code inline, dependency-free, and executed by workers.
- Make downstream bindings after the verifier consume only the verifier-accepted or exhaustion-finalized generation.

**Non-Goals:**

- General unbounded loops.
- Arbitrary graph cycles outside explicit verifier-controlled bounded reruns.
- Dependency installation or external package use inside verifier code.
- Automatically rolling back side effects from rejected generations.

## Verifier Shape

An Agent task may declare:

```yaml
verifier:
  max_iterations: 3
  on_exhausted_continue: true
  on_failure_rerun_task: bar
  code: |
    export default async function verify(ctx) {
      if (ctx.output.quality >= 0.9) {
        return { decision: "complete" };
      }
      return {
        decision: "continue",
        feedback: "Improve the evidence and tighten the final recommendation."
      };
    }
```

`on_exhausted_continue: true` means that if the verifier asks to continue after `max_iterations` is reached, RunHelm finalizes the latest schema-valid generation and proceeds through normal downstream bindings.

`on_exhausted_continue: false` means exhaustion fails the workflow.

`on_failure_rerun_task` identifies the task where the next generation starts. In `A -> B -> C -> D`, if task `D` declares `on_failure_rerun_task: B`, verifier `continue` creates a new generation for `B`, `C`, and `D`.

## Decisions

**1. Use verifier-controlled bounded backedges**

The verifier does not merely rerun its own Agent task. It declares the first task in the rerun slice. The orchestrator derives the affected slice from data bindings and only allows rerun targets that are upstream ancestors of the verifier task.

Alternative considered: introduce a nested `Loop` task kind. This is more explicit for loop bodies, but it adds a second workflow-definition language inside task definitions. A bounded backedge keeps workflows flat and reuses existing data bindings.

**2. Keep ordinary data bindings acyclic**

Workflow definitions still use acyclic data bindings. The verifier backedge is not represented as a normal `DataBinding`; it is a bounded control edge with explicit `max_iterations`. This preserves existing cycle validation while allowing bounded reruns.

**3. Materialize generation attempts for every task in the rerun slice**

Tasks in a rerun slice are persisted with stable generation IDs such as `b[2]`, `c[2]`, and `d[2]`. Each materialized instance retains its original task definition ID and generation index. Tasks outside the rerun slice keep their selected output from the previous workflow state.

**4. Resolve bindings by generation scope**

Within a rerun generation, bindings between tasks in the rerun slice consume outputs from the same generation. Bindings from outside the slice consume the latest selected output outside the slice. Downstream tasks after the verifier do not run until the verifier completes or exhaustion-finalizes a generation.

**5. Carry rerun feedback as dedicated execution metadata**

When a verifier returns `continue`, RunHelm persists the feedback and creates the next generation with orchestrator-owned loop context containing iteration, max iterations, latest feedback, and feedback history. This context is passed as dedicated execution metadata, not as an extra user-declared input.

Agent workers append the loop context to the prompt. Function and API tasks receive the execution request unchanged unless their executors explicitly support loop context later.

**6. Run verifier code in workers**

Verifier code should be inline, dependency-free, and executed by the worker after the verifier Agent output passes output schema validation. The verifier context should contain the verifier Agent output, generation index, max iterations, feedback history, and the selected upstream outputs needed by the verifier task.

The verifier must return `{ decision: "complete" }` or `{ decision: "continue", feedback: "<non-empty string>" }`.

**7. Preserve observability**

Rejected generations remain observable. A schema-valid task attempt in a rejected generation still has lifecycle status `Completed`; verifier status and generation selection are separate metadata. RunHelm does not roll back side effects from rejected generations.

**8. Normalize workflow and task IDs before persistence**

Workflow IDs and task IDs should be normalized to lowercase during registration and rejected unless they contain only ASCII alphanumeric characters. This reserves bracket syntax for generated attempt IDs such as `task[2]`.

## Risks / Trade-offs

- **Risk:** Rerun slices can be ambiguous in branching graphs. -> Mitigate by validating that `on_failure_rerun_task` is an upstream ancestor of the verifier and by defining generation scoping for all tasks reachable from the rerun task to the verifier.
- **Risk:** Re-executing tasks can repeat side effects. -> Mitigate by retaining rejected generations as audit history and documenting that workflow authors must make rerun slices side-effect-safe.
- **Risk:** Bounded backedges are less visually obvious than an explicit loop body. -> Mitigate with status/read APIs that expose generations and verifier decisions.
- **Risk:** Exhaustion with continue can allow lower-quality outputs through. -> Mitigate by requiring explicit `on_exhausted_continue: true` and recording the exit reason.

## Migration Plan

- Add verifier fields with serde defaults so existing serialized workflow definitions remain valid.
- Normalize workflow and task IDs at registration and reject non-alphanumeric IDs.
- Preserve current task instance keys for tasks outside verifier-controlled generation slices.
- Introduce materialized generation IDs only for tasks participating in a bounded rerun slice.
- Add generation state with defaults so older workflow instances remain readable.
- Roll back by rejecting workflows with verifier blocks while leaving existing workflows unaffected.
