## ADDED Requirements

### Requirement: Default Logical Task Workspace
The system SHALL provide one selected workspace directory for each task execution. By default, the selected workspace SHALL be a private logical-task workspace scoped to the workflow instance and logical task ID.

#### Scenario: Task uses default workspace
- **WHEN** a task definition does not declare `workspace.group_name`
- **THEN** the task execution uses a private workspace scoped to that workflow instance and logical task ID

#### Scenario: Later attempt reuses task workspace
- **WHEN** a later attempt is materialized for the same logical task within the same workflow instance
- **THEN** the task execution uses the same private workspace path as the earlier attempt

#### Scenario: Different task gets different workspace
- **WHEN** two task definitions in the same workflow instance do not declare the same `workspace.group_name`
- **THEN** each task uses a different selected workspace path

### Requirement: Workspace Group Override
The system SHALL allow a task to declare `workspace.group_name` to replace its default private workspace with a workflow-instance-scoped shared workspace group.

#### Scenario: Task declares workspace group
- **WHEN** a task declares `workspace.group_name: "repo"`
- **THEN** the task execution uses the selected workspace derived from the workflow instance id and group `repo`
- **THEN** the task does not receive its default private workspace path

#### Scenario: Multiple tasks declare same workspace group
- **WHEN** multiple tasks in the same workflow instance declare the same `workspace.group_name`
- **THEN** those tasks use the same selected workspace path for that group

#### Scenario: Task receives one workspace
- **WHEN** a task execution is dispatched
- **THEN** the task receives exactly one selected workspace path

### Requirement: Worker-Local Workspace Directories
The system SHALL create workspace directories under a configured worker-local workspace root using RunHelm-owned path construction.

#### Scenario: Private workspace key is derived
- **WHEN** a task without `workspace.group_name` is prepared for execution
- **THEN** the system derives a stable private workspace key from the workflow instance id and logical task id

#### Scenario: Group workspace key is derived
- **WHEN** a task with `workspace.group_name` is prepared for execution
- **THEN** the system derives a stable group workspace key from the workflow instance id and normalized workspace group name

#### Scenario: Private workspace path is created
- **WHEN** a task without `workspace.group_name` is prepared for execution
- **THEN** the system creates or resolves a directory under the worker-local workspace root for the workflow instance and logical task ID

#### Scenario: Group workspace path is created
- **WHEN** a task with `workspace.group_name` is prepared for execution
- **THEN** the system creates or resolves a directory under the worker-local workspace root from the derived group workspace key

#### Scenario: Workspace records timestamp marker
- **WHEN** the system creates a workspace directory
- **THEN** the workspace includes a `.timestamp` marker usable by later stale-directory cleanup

### Requirement: Workspace Manager Lifecycle
The system SHALL provide an orchestrator-side `WorkspaceManager` component for deriving workspace keys, creating selected workspaces, resolving workflow-instance group workspaces, and cleaning RunHelm-owned workspaces.

#### Scenario: Workspace manager creates task workspace
- **WHEN** a task execution is prepared
- **THEN** `WorkspaceManager` creates or resolves the selected workspace directory for that task

#### Scenario: Workspace manager resolves group key
- **WHEN** a later task in the same workflow instance declares the same normalized group name
- **THEN** `WorkspaceManager` derives the same group workspace key and resolves the same selected workspace path

#### Scenario: Workspace manager cleans workspace
- **WHEN** cleanup is requested for a RunHelm-owned workspace
- **THEN** `WorkspaceManager` attempts to remove that workspace directory

### Requirement: Workspace TTL Cleanup
The system SHALL support configurable workspace TTL cleanup through `WorkspaceManager`.

#### Scenario: Expired workspace is cleaned
- **WHEN** a RunHelm-owned workspace is older than the configured TTL
- **THEN** `WorkspaceManager` cleanup may remove that workspace

#### Scenario: Cleanup monitor runs periodically
- **WHEN** the `WorkspaceManager` TTL monitor is enabled
- **THEN** it wakes on a configured interval and attempts to clean expired workspaces

#### Scenario: TTL monitor is implemented last
- **WHEN** implementing the task workspace capability
- **THEN** workspace creation, executor payload propagation, and path validation are completed before the TTL monitor is added

### Requirement: Workspace Path Containment
The system SHALL ensure task file access is contained to the selected workspace path.

#### Scenario: Path resolves inside workspace
- **WHEN** task file access resolves to a path inside the selected workspace
- **THEN** the system allows the access subject to executor policy

#### Scenario: Path escapes workspace
- **WHEN** task file access resolves outside the selected workspace through an absolute path, `..` traversal, or symlink traversal
- **THEN** the system rejects the access
