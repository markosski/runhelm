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

### Requirement: Attempt-Aware Task Execution Payload
The task execution payload SHALL include task attempt generation metadata without requiring explicit Agent session policy metadata.

#### Scenario: Initial task attempt is dispatched
- **WHEN** the engine dispatches an initial task attempt
- **THEN** the payload includes `generation_index` equal to `1`

#### Scenario: Continuation task attempt is dispatched
- **WHEN** the engine dispatches a later task attempt
- **THEN** the payload includes that attempt's `generation_index`

#### Scenario: Non-Agent task is dispatched
- **WHEN** the engine dispatches a Function or API call task
- **THEN** the task does not require Agent session metadata

#### Scenario: Agent session reuse is disabled
- **WHEN** the engine dispatches an Agent task with `reuse_session` false
- **THEN** the payload does not require loading prior logical-task session history

### Requirement: Convention-Derived Agent Session Identity
The task execution contract SHALL use RunHelm-derived session keys for the normal Agent session reuse path rather than requiring workers to return opaque session identifiers.

#### Scenario: New reusable Agent session is created
- **WHEN** a worker creates a new durable Agent session while executing an initial reusable Agent task
- **THEN** the session is persisted under the session key derived from workflow instance ID and logical task ID

#### Scenario: Existing reusable Agent session is reused
- **WHEN** a worker executes an Agent task using an existing derived session key
- **THEN** the worker persists updates back to the same session key

### Requirement: Session Load Recovery Diagnostics
The task executor SHALL report missing or unreadable Agent sessions as diagnostics and continue with a fresh session.

#### Scenario: Expected session key cannot be loaded
- **WHEN** the worker derives an Agent session key that cannot be loaded
- **THEN** the worker logs a diagnostic that identifies the session-load problem

#### Scenario: Fresh replacement session is created
- **WHEN** the worker cannot load an expected existing Agent session
- **THEN** the worker creates a fresh session and continues execution
- **THEN** the worker prompts with the full task context plus the current human-input or verifier-feedback event

### Requirement: Workspace-Aware Task Execution Payload
The task execution payload SHALL include the selected workspace context for each dispatched task.

#### Scenario: Private workspace task is dispatched
- **WHEN** the engine dispatches a task that does not declare `workspace.group_name`
- **THEN** the payload includes the selected private workspace path
- **THEN** the payload identifies the workspace as private

#### Scenario: Group workspace task is dispatched
- **WHEN** the engine dispatches a task that declares `workspace.group_name`
- **THEN** the payload includes the selected group workspace path
- **THEN** the payload identifies the workspace as a group workspace and includes the group name

#### Scenario: Continuation attempt is dispatched
- **WHEN** the engine dispatches a later attempt for the same logical task
- **THEN** the payload includes the same selected workspace path used by the earlier attempt

### Requirement: Executor Workspace Exposure
Executors SHALL expose only the selected workspace path to task code.

#### Scenario: Docker task receives workspace
- **WHEN** a Docker-backed task is executed
- **THEN** the executor mounts or exposes the selected workspace path to the container

#### Scenario: Function task receives workspace
- **WHEN** a Function task is executed
- **THEN** the executor provides the selected workspace path to the function execution context

#### Scenario: Agent task receives workspace
- **WHEN** an Agent task is executed
- **THEN** the executor includes the selected workspace path in the Agent execution context or prompt

#### Scenario: Task does not receive multiple workspaces
- **WHEN** any task is executed
- **THEN** the executor does not expose both a private workspace and a group workspace to that task

### Requirement: Executor File Access Scope
Executors SHALL provide selected workspace path guidance without claiming strict filesystem containment for arbitrary task code in the initial implementation.

#### Scenario: Function file access guidance
- **WHEN** a Function task is executed
- **THEN** the executor provides the selected workspace path as the intended location for task file work
- **THEN** the executor does not claim to sandbox arbitrary JavaScript filesystem access to only that path

#### Scenario: Agent file access guidance
- **WHEN** an Agent task is executed
- **THEN** the executor includes the selected workspace path in the Agent prompt or execution context as the intended location for task file work
- **THEN** the executor does not claim to guarantee that the Agent only reads or writes under that path

#### Scenario: Docker reused worker container
- **WHEN** a Docker-backed task is executed by the reused worker container deployment
- **THEN** the worker receives the selected workspace path under the configured mounted workspace root
- **THEN** the executor does not claim per-task selected workspace mount isolation

#### Scenario: Future strict containment
- **WHEN** RunHelm adds owned file tools, per-task containers, or another sandbox
- **THEN** that future design may validate file paths against the selected workspace before filesystem access
