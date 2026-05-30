## Context

RunHelm models a workflow definition as task definitions plus data bindings, and a workflow instance as materialized `TaskInstance` attempts. A normal workflow creates generation-1 attempts such as `a[1]` and `b[1]`; each attempt records its logical task definition ID and the concrete upstream attempts consumed through `input_mapping`.

Some workflows need bounded revision across more than one task. For example, a workflow may run `A -> B -> C -> D`, where verifier task `D` reviews the result and may decide that the workflow should go back to `B`. In that case RunHelm should re-execute `B -> C -> D` as a new bounded generation while preserving `A`.

The bounded-loop control point is explicit in workflow definition, but it is not a separate nested `Loop` body. A `control.verifier` block on any task kind defines a bounded control edge to itself or to a previous task in the dependency chain.

## Goals / Non-Goals

**Goals:**

- Add `control.verifier` that lets a task gate progress after it runs.
- Let a verifier rerun itself or a previous task in its upstream chain, causing the affected path back to the verifier to execute again.
- Persist materialized task attempts such as `b[1]`, `c[1]`, `d[1]`, then `b[2]`, `c[2]`, `d[2]`.
- Preserve ordinary acyclic workflow behavior when verifier controls are absent.
- Make verifier tasks return the decision directly using an injected decision schema.
- Make downstream bindings after the verifier consume only the accepted or exhaustion-finalized generation.

**Non-Goals:**

- General unbounded loops.
- Arbitrary graph cycles outside explicit verifier-controlled bounded reruns.
- Inline verifier code or verifier-specific dependency installation.
- Automatically rolling back side effects from rejected generations.

## Verifier Shape

A task may declare:

```yaml
control:
  verifier:
    max_iterations: 3
    on_exhausted_continue: true
    rerun_from_task_id: bar
```

The verifier task must not declare its own `output_schema`. During workflow registration, RunHelm injects the decision schema requiring `decision: "complete" | "continue"` and allowing `feedback`.

The verifier task output is the verifier decision:

```json
{ "decision": "continue", "feedback": "Improve the evidence and tighten the final recommendation." }
```

The verifier decision is control output, not corrected business data. If a Function or Agent needs to transform data, model that as a normal task; use `control.verifier` only to decide whether the selected rerun slice should run again.

`on_exhausted_continue: true` means that if the verifier asks to continue after `max_iterations` is reached, RunHelm finalizes the latest schema-valid generation and proceeds through normal downstream bindings.

`on_exhausted_continue: false` means exhaustion fails the workflow.

`rerun_from_task_id` identifies the task where the next generation starts. In `A -> B -> C -> D`, if task `D` declares `rerun_from_task_id: B`, verifier `continue` creates a new generation for `B`, `C`, and `D`. If omitted, the verifier self-reruns.

## Decisions

**1. Use verifier-controlled bounded backedges**

The verifier does not need to rerun only itself. It can declare the first task in the rerun slice. The orchestrator derives the affected slice from data bindings and only allows rerun targets that are the verifier itself or upstream ancestors of the verifier task.

Alternative considered: introduce a nested `Loop` task kind. This is more explicit for loop bodies, but it adds a second workflow-definition language inside task definitions. A bounded backedge keeps workflows flat and reuses existing data bindings.

**2. Keep ordinary data bindings acyclic**

Workflow definitions still use acyclic data bindings. The verifier backedge is not represented as a normal `DataBinding`; it is a bounded control edge with explicit `max_iterations`. This preserves existing cycle validation while allowing bounded reruns.

**3. Inject verifier decision schema**

Verifier output is just task output. A verifier task with `control.verifier` must not declare a custom `output_schema`; registration injects the standard decision schema. This keeps normal output validation as the gate before verifier decisions are applied.

Verifier control is task-kind-neutral. Function verifiers are useful for deterministic checks over upstream outputs, Agent verifiers are useful for judgment-heavy review, and API verifiers are allowed when workflow authors accept the side-effect and reliability trade-offs.

**4. Materialize attempts for every task execution**

Workflow startup creates generation-1 attempts for the static graph. Later verifier retries create additional attempts only for the rerun slice. Each materialized instance retains its original task definition ID, generation index, satisfaction status, and input mapping.

**5. Resolve bindings by satisfaction and generation scope**

Within a rerun generation, bindings between tasks in the rerun slice consume outputs from the same generation. Bindings from outside the slice consume the latest satisfied output outside the slice. Downstream tasks after the verifier do not run until a verifier generation is accepted or exhaustion-finalized.

**6. Carry rerun feedback as dedicated execution metadata**

When a verifier returns `continue`, RunHelm persists the feedback and creates the next generation with orchestrator-owned loop context containing generation, max iterations, ordered feedback history, and previous same-task output. This context is passed as dedicated execution metadata, not as an extra user-declared input.

Agent workers append loop context to the prompt. Function and API tasks receive the execution metadata on the normal executor path; executors may decide how much of that metadata to use.

**7. Preserve observability**

Rejected generations remain observable. A schema-valid task attempt in a rejected generation still has lifecycle status `Completed`; `satisfaction_status` records whether the attempt may satisfy downstream bindings. RunHelm does not roll back side effects from rejected generations.

**8. Normalize workflow and task definition IDs before persistence**

Workflow definition IDs and task definition IDs are normalized to lowercase during registration and rejected unless they contain only ASCII alphanumeric characters. Generated task attempt IDs such as `task[2]` are orchestrator-owned runtime identifiers, not user-authored definition IDs.

**9. Reject overlapping verifier slices**

A task can belong to at most one verifier-controlled rerun slice. Back-to-back verifier regions are allowed, but overlapping or nested slices are rejected to keep generation-scoped binding resolution unambiguous.

## Risks / Trade-offs

- **Risk:** Rerun slices can be ambiguous in branching graphs. -> Mitigate by validating `rerun_from_task_id`, rejecting overlapping slices, and recording `input_mapping` for every attempt.
- **Risk:** Re-executing tasks can repeat side effects. -> Mitigate by retaining rejected generations as audit history and documenting that workflow authors must make rerun slices side-effect-safe.
- **Risk:** Bounded backedges are less visually obvious than an explicit loop body. -> Mitigate with status/read APIs that expose attempts, verifier states, satisfaction, and input lineage.
- **Risk:** Exhaustion with continue can allow lower-quality outputs through. -> Mitigate by requiring explicit `on_exhausted_continue: true` and recording the exit reason.

## Migration Plan

- Add `control` and verifier fields with serde defaults so existing serialized workflow definitions remain valid.
- Normalize workflow definition and task definition IDs at registration and reject non-alphanumeric definition IDs.
- Store materialized attempts under stable `task[generation]` IDs.
- Add verifier generation state, satisfaction state, and input mapping with defaults so older workflow instances remain readable.
- Roll back by rejecting workflows with `control.verifier` while leaving existing workflows unaffected.
