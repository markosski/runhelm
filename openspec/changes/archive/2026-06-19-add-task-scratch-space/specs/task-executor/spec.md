## ADDED Requirements

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
