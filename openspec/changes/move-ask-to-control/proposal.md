## Why

The current agent-only `ask` flag sits outside the task control model, while verifier-driven retries already use `control` and task attempts. Moving ask semantics into control makes human input another explicit orchestration control path and aligns it with the generation/attempt model.

## What Changes

- Add a `control.ask` capability that allows eligible tasks to return `InputNeeded`.
- Treat human input responses as a cause for creating a new attempt of the same logical task, preserving the original `InputNeeded` attempt for lineage.
- Align ask handling with verifier-style orchestration control rather than keeping it as an agent-kind-specific field.
- Define how human input attempts interact with task status, satisfaction, input mappings, and downstream eligibility.
- Replace the existing `kind.Agent.ask` behavior with `control.ask` before workflow definitions are treated as stable external contracts.

## Capabilities

### New Capabilities

### Modified Capabilities
- `workflow-dataflow-engine`: Add task-control ask semantics, including `InputNeeded` handling and human-input-triggered task attempts within the workflow dataflow lifecycle.

## Impact

- `orchestrator/src/core/models.rs` task definitions and control configuration.
- `orchestrator/src/core/engine.rs` task execution, `InputNeeded` handling, attempt materialization, and downstream readiness.
- `orchestrator/src/core/workflow_service.rs` workflow registration validation and task result/status reporting.
- Agent executor behavior that currently relies on ask being configured inside the agent task kind.
- Tests covering ask behavior, verifier/control interactions, task attempts, and input lineage.
- Documentation under `docs/` describing the updated ask/control model and its relationship to attempts/generations.
