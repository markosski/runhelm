# Capability: workflow-dataflow-engine

## Purpose
Defines how the orchestrator constructs and executes a workflow DAG based on data dependencies, transitioning tasks dynamically as their inputs are satisfied by upstream task outputs.

## Requirements

### Requirement: Task Instance Lifecycle Management
The orchestrator SHALL transition `TaskInstance`s through a strictly defined lifecycle: `Pending` -> `Running` -> (`Completed` or `Failed`).

#### Scenario: Valid inputs trigger execution
- **WHEN** all `input_schemas` of a `Pending` task are satisfied by upstream data bindings
- **THEN** the task status transitions from `Pending` to `Running`

#### Scenario: Workflow initialization
- **WHEN** a `WorkflowInstance` is initialized from a `WorkflowDef`
- **THEN** all tasks are instantiated as `Pending`, and any tasks with no data bindings immediately transition to `Running`

### Requirement: Data Binding Resolution
The orchestrator SHALL construct the execution DAG based exclusively on the `DataBinding`s defined in the `WorkflowDef`.

#### Scenario: Sequential propagation
- **WHEN** Task A completes successfully
- **THEN** the output of Task A is mapped to the input payload of Task B according to the defined `DataBinding`

#### Scenario: Fan-In propagation
- **WHEN** Task C requires inputs from both Task A and Task B
- **THEN** Task C SHALL NOT transition to `Running` until both Task A and Task B have successfully completed and populated their respective input bindings on Task C
