# Capability: task-executor

## Purpose
Defines the `ExecutorPort` interface — the contract between the `WorkflowEngine` and any concrete task execution backend. The engine delegates all task execution through this port; it never performs execution directly.

## Requirements

### Requirement: Executor Port Interface
The system SHALL expose an async trait `ExecutorPort` with a single method `execute` that accepts a task definition and its resolved inputs, and returns a JSON value or an error.

#### Scenario: Successful execution
- **WHEN** `ExecutorPort::execute` is called with a valid `TaskDef` and a slice of resolved JSON input values
- **THEN** it SHALL return `Ok(serde_json::Value)` representing the task's raw output

#### Scenario: Execution failure
- **WHEN** `ExecutorPort::execute` encounters an error (e.g., container crash, network failure, non-zero exit code)
- **THEN** it SHALL return `Err(anyhow::Error)` with a human-readable description of the failure

### Requirement: Engine Integration
The `WorkflowEngine` SHALL accept an `Arc<dyn ExecutorPort + Send + Sync>` at construction time and SHALL call it for every task transition from `Running` to a terminal state.

#### Scenario: Engine calls executor for each ready task
- **WHEN** the engine identifies a `Pending` task whose data-binding inputs are all satisfied
- **THEN** it SHALL call `ExecutorPort::execute` with that task's `TaskDef` and the resolved input array before performing schema validation

#### Scenario: Executor output is passed to schema validation
- **WHEN** `ExecutorPort::execute` returns `Ok(output)`
- **THEN** the engine SHALL validate `output` against `TaskDef.output_schema` before marking the task `Completed` or `Failed`

### Requirement: Executor Does Not Validate Output
The `ExecutorPort` implementation SHALL NOT perform JSON Schema validation on the value it returns. Schema validation is the exclusive responsibility of the engine.

#### Scenario: Raw output returned without validation
- **WHEN** an executor produces a JSON value
- **THEN** it SHALL return that value as-is, even if it does not conform to `TaskDef.output_schema`
