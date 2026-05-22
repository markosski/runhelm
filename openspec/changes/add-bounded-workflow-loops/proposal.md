## Why

RunHelm needs a first-class way for a workflow to revise a bounded slice of prior work when a verifier Agent decides the current result is not good enough. For example, a workflow may run `A -> B -> C -> D`, where `D` evaluates the result and asks RunHelm to rerun from `B`, producing `B[2] -> C[2] -> D[2]` before downstream tasks continue.

## What Changes

- Add an Agent verifier block with `max_iterations`, `on_exhausted_continue`, `on_failure_rerun_task`, and dependency-free verifier `code`.
- Treat verifier `continue` as a bounded backedge to the configured previous task instead of rerunning only the verifier Agent.
- Materialize rerun-slice generations as distinct runtime attempts, such as `b[1]`, `c[1]`, `d[1]`, then `b[2]`, `c[2]`, `d[2]`.
- Add a dependency-free verifier code contract returning `decision: "continue" | "complete"` and optional feedback.
- Add orchestrator-owned loop context for repeated generations, including iteration metadata, latest feedback, and feedback history.
- Add configurable exhaustion behavior so a verifier can either fail the workflow or continue with the latest schema-valid generation.
- Resolve downstream bindings after the verifier only after a generation is accepted or finalized by exhaustion policy.

## Capabilities

### New Capabilities

- `workflow-bounded-loops`: Defines Agent verifier blocks, verifier decisions, bounded backedges, exhaustion policy, loop context, generation materialization, selected-generation binding, and observability.

### Modified Capabilities

- `workflow-dataflow-engine`: Extends workflow execution semantics from one task instance per task definition to support verifier-controlled generations whose selected outputs satisfy downstream bindings.

## Impact

- **Core Models:** Agent task definitions gain verifier configuration; workflow instances gain persisted generation state and feedback history.
- **Execution Engine:** Scheduling must execute Agent verifier code after verifier Agent output, create new rerun-slice generations on `continue`, and expose only accepted/finalized generations to downstream bindings.
- **Status and Result APIs:** Workflow status and task result lookup must expose materialized generation attempts while preserving normal task IDs for non-rerun tasks.
- **Compatibility:** Existing workflows without Agent verifier blocks remain valid and continue to execute as ordinary dataflow DAGs.
