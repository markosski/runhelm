## Why

RunHelm needs a first-class way for unpredictable Agent tasks to refine their output through bounded verification feedback before downstream workflow tasks consume it. Modeling the verifier as a normal Function task is confusing because the orchestrator depends on special verifier semantics; the verifier should instead be an Agent task capability that gates task completion.

## What Changes

- Add an Agent-only verifier block that runs immediately after each Agent attempt produces schema-valid output.
- Materialize verified Agent attempts as distinct runtime attempts, such as `implementchange[1]` and `implementchange[2]`, while preserving ordinary task behavior for tasks without verifiers.
- Add a dependency-free verifier code contract returning `decision: "continue" | "complete"` and optional feedback.
- Add orchestrator-owned loop context for repeated Agent attempts, including iteration metadata, latest feedback, and feedback history.
- Add configurable exhaustion behavior so a verified Agent can either fail the workflow or continue with the highest available attempt when its iteration limit is reached.
- Resolve downstream bindings from verified Agent tasks only after an attempt is accepted or finalized by exhaustion policy.

## Capabilities

### New Capabilities

- `workflow-bounded-loops`: Defines Agent verifier blocks, verifier decisions, exhaustion policy, loop context, attempt materialization, accepted attempt binding, and observability.

### Modified Capabilities

- `workflow-dataflow-engine`: Extends workflow execution semantics from one task instance per task definition to support verified Agent generations whose accepted attempt satisfies downstream bindings.

## Impact

- **Core Models:** Agent task definitions gain verifier configuration; workflow instances gain persisted verified attempt state and feedback history.
- **Execution Engine:** Scheduling must execute Agent verifier code after Agent output, create new Agent attempts on `continue`, and expose only accepted/finalized attempts to downstream bindings.
- **Status and Result APIs:** Workflow status and task result lookup must expose materialized Agent attempts while preserving normal task IDs for non-verified tasks.
- **Compatibility:** Existing workflows without Agent verifier blocks remain valid and continue to execute as ordinary dataflow DAGs.
