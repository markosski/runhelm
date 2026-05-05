## Why

RunHelm needs a flexible, natively parallelizable execution model for agentic workflows. By adopting a Dataflow Architecture instead of a linear step-by-step engine, tasks execute dynamically as their data dependencies are met. This also creates a strict schema-validation boundary to safeguard against unpredictable LLM agent outputs.

## What Changes

- Introduce the concept of Definitions (blueprints) vs. Instances (live executions).
- Define `WorkflowDef`, `TaskDef`, and `DataBinding` to construct execution DAGs based on inputs.
- Define `WorkflowInstance` and `TaskInstance` to hold state, side effects, and live data payloads.
- Update the execution engine loop to process Tasks as `Pending` -> `Running` -> `Completed`/`Failed` based on data readiness and runtime JSON schema validation.

## Capabilities

### New Capabilities
- `workflow-dataflow-engine`: The core mechanism for resolving task dependencies and executing tasks as their inputs become satisfied.
- `task-schema-validation`: Strict runtime validation of task outputs against predefined JSON schemas.

### Modified Capabilities

## Impact

- **Core Models:** Major additions to `src/core/models.rs` replacing the placeholder implementations.
- **Storage Port:** Updates to `StoragePort` to handle definitions separately from instances.
- **Execution Engine:** A new execution loop will need to be implemented within the orchestrator to evaluate the DAG dynamically.
