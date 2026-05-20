## Context

RunHelm currently models a workflow definition as task definitions plus data bindings, and a workflow instance as a map from task definition ID to one `TaskInstance`. A task is considered complete once execution succeeds and output schema validation passes, so downstream bindings can consume that output immediately.

The original bounded-loop design used workflow-level loop bodies and a separate verifier task. During design discussion, that proved less principled: a verifier represented as a normal Function task would have special orchestrator semantics hidden behind an ordinary task output. The cleaner model is to make verification a capability of Agent tasks, because Agent output is the unpredictable output that needs bounded refinement.

## Goals / Non-Goals

**Goals:**

- Add an Agent-only verifier block that gates Agent task completion.
- Let a verified Agent produce multiple materialized attempts before one attempt is accepted.
- Persist enough attempt, verifier, and feedback state to resume, observe, and reconstruct each generation.
- Make downstream bindings consume the highest accepted or exhaustion-finalized Agent attempt.
- Preserve existing workflow behavior when Agent verifier blocks are absent.

**Non-Goals:**

- Verifier blocks on Function or API tasks in v1.
- Dependency installation or external package use inside verifier code.
- Separate verifier tasks with special orchestrator interpretation.
- General conditional branching or workflow-level loop bodies.
- Provider-specific Agent session continuation as the primary memory model.

## Decisions

**1. Put verifier configuration on Agent tasks**

The Agent task definition should support a `verifier` block with `max_iterations`, `on_exhausted`, `loop_context_input_index`, and inline verifier `code`. The block is only valid for `Agent` tasks.

Alternative considered: keep a separate verifier Function task. This was rejected because the orchestrator would treat that task's output as control flow, making it not truly an ordinary Function task.

**2. Run verifier code in workers and keep it dependency-free**

Verifier code should be inline, dependency-free, and executed by the worker after Agent output schema validation succeeds. The worker already owns task execution and the TypeScript runtime, so the Rust orchestrator should not gain a JavaScript execution surface for verifier code.

The verifier context should contain only `output`, `attempt`, `max_iterations`, and `feedback_history`. It intentionally omits prior verifier result to keep the verifier contract small. The verifier must return `{ decision: "complete" }` or `{ decision: "continue", feedback: "<non-empty string>" }`.

Alternative considered: support Function-style dependencies in verifier code. This was rejected to keep verification fast, local, and simple; complex verification can still be modeled as normal workflow tasks.

**3. Materialize Agent attempts with stable generation IDs**

A verified Agent attempt should be stored as `task_id[1]`, `task_id[2]`, and so on, while retaining the original task definition ID. Rejected attempts remain observable but do not satisfy downstream bindings. A schema-valid rejected attempt still has lifecycle status `Completed`; verifier status such as accepted, rejected, finalized, or exhausted is separate metadata. Tasks without verifiers should have no verifier metadata.

Alternative considered: overwrite the same `TaskInstance` on retry. This was rejected because it loses history and weakens auditability for agentic workflows.

**4. Persist logical verified Agent state beside materialized attempts**

Verified Agent attempts should remain in `WorkflowInstance.tasks` keyed by materialized attempt ID. A separate verified Agent state index keyed by logical task ID should track attempt IDs, latest attempt, accepted or finalized attempt, feedback history, verifier status, and exit reason.

Task result lookup should support both identities. Looking up `implementchange[1]` returns that exact historical attempt. Looking up `implementchange` resolves to the accepted or exhaustion-finalized attempt and includes the resolved attempt ID in the response.

Alternative considered: store attempts nested under one logical task instance. This was rejected because existing status and task result APIs already operate on task-instance IDs, and first-class attempts preserve direct observability.

**5. Treat accepted/finalized attempt output as the binding source**

For a source task with verified attempts, data binding resolution should use the highest accepted attempt. If exhaustion policy is `continue`, the highest available attempt is finalized and becomes the binding source. If exhaustion policy is `fail`, no downstream binding is released.

Alternative considered: propagate each attempt output immediately. This was rejected because downstream tasks could consume rejected Agent output.

**6. Use explicit replay loop context for Agent memory**

When the verifier returns `continue`, the next Agent attempt receives orchestrator-owned loop context at the configured input index. The context includes iteration, max iterations, latest feedback, and feedback history. This state is derived from persisted workflow state, not user-provided trigger input.

Alternative considered: persist and resume provider-specific Agent sessions. This can be added later as an execution optimization, but it should not be required for correctness or reproducibility.

**7. Keep non-verified workflows backward compatible**

Agent tasks without verifiers and all non-Agent task kinds should retain current lifecycle and binding behavior. Existing status clients should continue to see ordinary task IDs for tasks without verified attempts.

**8. Normalize workflow and task IDs before persistence**

Workflow IDs and task IDs should be normalized to lowercase during registration and rejected unless they contain only ASCII alphanumeric characters. This reserves bracket syntax for generated attempt IDs such as `implementchange[1]` and avoids escaping rules in storage, binding resolution, and API lookup.

## Risks / Trade-offs

- **Risk:** Attempt IDs introduce a second identifier shape for verified Agent tasks. -> Mitigate by documenting materialized IDs and retaining the original task definition ID on each attempt.
- **Risk:** Verifier code can become a hidden place for complex business logic. -> Mitigate by disallowing dependencies and positioning verifier code as simple accept/continue logic.
- **Risk:** Exhaustion with `continue` may allow lower-quality outputs through a verifier gate. -> Mitigate by making exhaustion policy explicit and recording the exit reason.
- **Risk:** Loop context input can conflict with existing Agent schemas. -> Mitigate by making `loop_context_input_index` explicit and requiring matching input schema when used.
- **Risk:** Startup recovery must handle materialized Agent attempts left `Running` or verifying. -> Mitigate by applying recovery to all non-terminal materialized attempts.
- **Risk:** Rejected Agent attempts may have already produced side effects. -> Mitigate by documenting that RunHelm does not roll back side effects; workflow authors must account for side-effect safety.

## Migration Plan

- Add verifier fields with serde defaults so existing serialized workflow definitions remain valid.
- Normalize workflow and task IDs at registration and reject non-alphanumeric IDs.
- Preserve current task instance keys for tasks without verifier-generated attempts.
- Introduce materialized attempt IDs only for Agent tasks with verifier blocks.
- Add logical verified Agent state with defaults so older workflow instances without that map remain readable.
- Roll back by rejecting workflows with verifier blocks while leaving existing workflows unaffected.
