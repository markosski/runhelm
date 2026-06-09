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

### Requirement: Executor File Tool Workspace Validation
Executors that provide file read or write tools SHALL validate tool-call paths against the selected workspace path before accessing the filesystem.

#### Scenario: File tool accesses workspace file
- **WHEN** a file tool call resolves to a path inside the selected workspace
- **THEN** the executor allows the file operation subject to normal tool policy

#### Scenario: File tool attempts traversal
- **WHEN** a file tool call resolves outside the selected workspace through `..` traversal
- **THEN** the executor rejects the file operation

#### Scenario: File tool attempts absolute path escape
- **WHEN** a file tool call uses an absolute path outside the selected workspace
- **THEN** the executor rejects the file operation

#### Scenario: File tool attempts symlink escape
- **WHEN** a file tool call resolves through a symlink to a path outside the selected workspace
- **THEN** the executor rejects the file operation
