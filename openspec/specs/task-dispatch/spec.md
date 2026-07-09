# Capability: task-dispatch

## Purpose
Defines the `TaskDispatchPort` interface, the narrow contract between the `WorkflowEngine` and task dispatch. The engine delegates runnable task attempts through this port and does not know how workers are registered, how workers claim tasks, or how task results return.

## Requirements

### Requirement: Task Dispatch Port Interface
The system SHALL expose an async trait `TaskDispatchPort` with a single method `dispatch_task` that accepts a workflow instance ID, task definition, resolved inputs, execution metadata, and dispatch constraints.

#### Scenario: Successful task result
- **WHEN** `TaskDispatchPort::dispatch_task` completes with task output
- **THEN** it SHALL return `ExecutionResult::Success` with the raw JSON output

#### Scenario: Human input needed
- **WHEN** task execution requests human input
- **THEN** it SHALL return `ExecutionResult::InputNeeded` with the input request description

#### Scenario: Task failure
- **WHEN** task execution fails
- **THEN** it SHALL return `ExecutionResult::Failure` with a human-readable reason

#### Scenario: Dispatch infrastructure error
- **WHEN** dispatch cannot be completed because the dispatch path is cancelled or otherwise unavailable
- **THEN** it SHALL return `Err(anyhow::Error)` with a human-readable description

### Requirement: Engine Integration
The `WorkflowEngine` SHALL accept an `Arc<dyn TaskDispatchPort + Send + Sync>` at construction time and SHALL call it for every runnable task attempt.

#### Scenario: Engine dispatches each ready task
- **WHEN** the engine identifies a `Pending` task whose data-binding inputs are all satisfied
- **THEN** it SHALL call `TaskDispatchPort::dispatch_task` with that task's resolved payload before performing output schema validation

#### Scenario: Dispatcher output is passed to schema validation
- **WHEN** `TaskDispatchPort::dispatch_task` returns `ExecutionResult::Success(output)`
- **THEN** the engine SHALL validate `output` against `TaskDef.output_schema` before marking the task `Completed` or `Failed`

### Requirement: Dispatcher Does Not Validate Output
The `TaskDispatchPort` implementation SHALL NOT perform JSON Schema validation on successful task output. Schema validation is the exclusive responsibility of the engine.

#### Scenario: Raw output returned without validation
- **WHEN** a dispatcher receives successful worker output
- **THEN** it SHALL return that value as-is, even if it does not conform to `TaskDef.output_schema`

### Requirement: Attempt-Aware Dispatch Payload
The task dispatch payload SHALL include task attempt generation metadata without requiring explicit Agent session policy metadata.

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
The worker SHALL report missing or unreadable Agent sessions as diagnostics and continue with a fresh session.

#### Scenario: Expected session key cannot be loaded
- **WHEN** the worker derives an Agent session key that cannot be loaded
- **THEN** the worker logs a diagnostic that identifies the session-load problem

#### Scenario: Fresh replacement session is created
- **WHEN** the worker cannot load an expected existing Agent session
- **THEN** the worker creates a fresh session and continues execution
- **THEN** the worker prompts with the full task context plus the current human-input or verifier-feedback event

### Requirement: Workspace-Aware Dispatch Payload
The task dispatch payload SHALL include the selected workspace context for each dispatched task.

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

### Requirement: Workspace Exposure
Workers SHALL receive the selected workspace path and expose only that selected workspace path to task code as the intended task working location.

#### Scenario: Worker-dispatched task receives workspace
- **WHEN** a task is dispatched to a worker
- **THEN** the worker receives the selected workspace path under its configured workspace root

#### Scenario: Function task receives workspace
- **WHEN** a Function task is executed by a worker
- **THEN** the function execution context includes the selected workspace path

#### Scenario: Agent task receives workspace
- **WHEN** an Agent task is executed by a worker
- **THEN** the Agent execution context or prompt includes the selected workspace path

#### Scenario: Task does not receive multiple workspaces
- **WHEN** any task is executed
- **THEN** the worker does not expose both a private workspace and a group workspace to that task

### Requirement: File Access Scope
Workers SHALL provide selected workspace path guidance without claiming strict filesystem containment for arbitrary task code in the initial implementation.

#### Scenario: Function file access guidance
- **WHEN** a Function task is executed
- **THEN** the worker provides the selected workspace path as the intended location for task file work
- **THEN** the worker does not claim to sandbox arbitrary JavaScript filesystem access to only that path

#### Scenario: Agent file access guidance
- **WHEN** an Agent task is executed
- **THEN** the worker includes the selected workspace path in the Agent prompt or execution context as the intended location for task file work
- **THEN** the worker does not claim to guarantee that the Agent only reads or writes under that path

#### Scenario: Reused worker deployment
- **WHEN** a task is executed by a reused worker process
- **THEN** the worker receives the selected workspace path under the configured mounted workspace root
- **THEN** the system does not claim per-task selected workspace mount isolation

#### Scenario: Future strict containment
- **WHEN** RunHelm adds owned file tools, per-task containers, or another sandbox
- **THEN** that future design may validate file paths against the selected workspace before filesystem access
