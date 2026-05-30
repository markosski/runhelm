## Why

RunHelm needs a first-class way for a workflow to revise a bounded slice of prior work when a verifier task decides the current result is not good enough. For example, a workflow may run `A -> B -> C -> D`, where `D` evaluates the result and asks RunHelm to rerun from `B`, producing `B[2] -> C[2] -> D[2]` before downstream tasks continue.

## What Changes

- Add `control.verifier` to task definitions with `max_iterations`, `on_exhausted_continue`, and optional `rerun_from_task_id`.
- Treat verifier task output as the verifier decision; RunHelm injects the verifier decision `output_schema` and rejects custom verifier `output_schema` declarations.
- Treat verifier `continue` as a bounded backedge to `rerun_from_task_id`, or as a self-rerun when `rerun_from_task_id` is omitted.
- Materialize runtime attempts as stable `task[1]`, `task[2]`, etc. records with original task definition ID, satisfaction state, generation index, and input lineage.
- Track rejected generations as observable completed attempts that are unsatisfied for downstream binding.
- Add orchestrator-owned loop context for repeated generations, including iteration metadata, ordered feedback history, and previous same-task output.
- Add configurable exhaustion behavior so a verifier can either fail the workflow or continue with the latest schema-valid generation.
- Resolve downstream bindings after the verifier only after a generation is accepted or finalized by exhaustion policy.

## Capabilities

### New Capabilities

- `workflow-bounded-loops`: Defines verifier controls, verifier decisions, bounded backedges, exhaustion policy, loop context, generation materialization, selected-generation binding, satisfaction state, input lineage, and observability.

### Modified Capabilities

- `workflow-dataflow-engine`: Extends workflow execution semantics from one materialized attempt per task definition to support verifier-controlled generations whose satisfied outputs feed downstream bindings.

## Impact

- **Core Models:** Task definitions gain optional `control.verifier`; workflow instances gain persisted verifier generation state, feedback history, satisfaction state, and input mapping.
- **Execution Engine:** Scheduling must parse verifier task outputs, create new rerun-slice generations on `continue`, mark accepted/finalized generations as satisfied, and expose only satisfied attempts to downstream bindings.
- **Status and Result APIs:** Workflow status and task result lookup must expose materialized attempts, satisfaction state, input mapping, verifier metadata, and verifier summary state.
- **Compatibility:** Existing workflows without `control.verifier` remain valid and execute as ordinary dataflow DAGs using generation-1 materialized attempts.
