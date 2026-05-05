# Capability: task-schema-validation

## Purpose
Defines the requirement for the orchestrator to strictly validate task outputs against their declared JSON schemas before propagating data downstream, ensuring type safety across the workflow DAG.

## Requirements

### Requirement: Strict Output Schema Validation
The orchestrator SHALL validate the JSON output of every `TaskInstance` against the `output_schema` defined in its corresponding `TaskDef` before propagating the data.

#### Scenario: Valid JSON Output
- **WHEN** a `TaskInstance` finishes execution and provides a JSON output that strictly satisfies the `output_schema`
- **THEN** the `TaskInstance` is marked as `Completed` and its data is allowed to flow downstream

#### Scenario: Invalid JSON Output
- **WHEN** a `TaskInstance` finishes execution and provides a JSON output that fails to satisfy the `output_schema` (e.g., missing required fields, wrong types)
- **THEN** the `TaskInstance` is marked as `Failed`, its data is NOT propagated, and any dependent tasks remain blocked or are marked as cancelled
