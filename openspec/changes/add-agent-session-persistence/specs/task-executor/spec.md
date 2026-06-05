## ADDED Requirements

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
