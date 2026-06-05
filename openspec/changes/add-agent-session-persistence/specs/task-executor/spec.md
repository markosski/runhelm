## ADDED Requirements

### Requirement: Session-Aware Agent Execution Payload
The task execution payload SHALL support Agent session continuation metadata without requiring non-Agent task kinds to use sessions.

#### Scenario: Initial reusable Agent task has a derived session key
- **WHEN** the engine dispatches an initial Agent task attempt with `reuse_session` true
- **THEN** the payload includes the session key derived from workflow instance ID and logical task ID
- **THEN** the payload marks that an existing session is not required

#### Scenario: Reusable Agent continuation has a required session key
- **WHEN** the engine dispatches an Agent continuation attempt with `reuse_session` true
- **THEN** the payload includes the session key derived from workflow instance ID and logical task ID
- **THEN** the payload marks that an existing session is required

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
- **THEN** the session is persisted under the session key supplied in the payload

#### Scenario: Existing reusable Agent session is reused
- **WHEN** a worker executes an Agent task using an existing session key
- **THEN** the worker persists updates back to the same session key

### Requirement: Session Load Failure Reporting
The task executor SHALL report missing or unreadable Agent sessions as execution failures with a clear error.

#### Scenario: Required session key cannot be loaded
- **WHEN** the worker receives an Agent task payload with a required existing session key that cannot be loaded
- **THEN** the worker returns a task execution failure that identifies the session-load problem

#### Scenario: Blank continuation session is not created
- **WHEN** the worker cannot load a required existing Agent session
- **THEN** the worker MUST NOT create a new empty session and continue execution
