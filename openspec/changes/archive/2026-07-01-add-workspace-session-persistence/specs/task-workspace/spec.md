## ADDED Requirements

### Requirement: Workflow Host Pinning
The system SHALL pin every workflow instance to one worker host when the workflow instance is created for execution.

#### Scenario: Workflow is pinned on first task assignment
- **WHEN** a workflow instance is created for execution
- **THEN** the system persists a workflow-instance host pin to that host ID

#### Scenario: Group workspace uses workflow pin
- **WHEN** a task with `workspace.group_name` is prepared for execution
- **AND** the workflow instance already has a host pin
- **THEN** the task execution requires the host ID from the workflow-instance host pin

#### Scenario: Workflow pin survives pause
- **WHEN** a workflow enters `InputNeeded`
- **AND** a host pin exists for the workflow instance
- **THEN** the host pin remains available for resumed execution

## MODIFIED Requirements

### Requirement: Default Logical Task Workspace
The system SHALL provide one selected workspace directory for each task execution. By default, the selected workspace SHALL be a private logical-task workspace scoped to the workflow instance and logical task ID on the workflow instance's pinned host.

#### Scenario: Task uses default workspace
- **WHEN** a task definition does not declare `workspace.group_name`
- **THEN** the task execution uses a private workspace scoped to that workflow instance and logical task ID

#### Scenario: Later attempt reuses task workspace host
- **WHEN** a later attempt is materialized for the same logical task within the same workflow instance
- **THEN** the task execution uses the same pinned host as the earlier attempt

#### Scenario: Different task gets different workspace
- **WHEN** two task definitions in the same workflow instance do not declare the same `workspace.group_name`
- **THEN** each task uses a different selected workspace path

### Requirement: Worker-Local Workspace Directories
The system SHALL create workspace directories under the executing worker's configured worker-local workspace root using RunHelm-owned path construction.

#### Scenario: Private workspace key is derived
- **WHEN** a task without `workspace.group_name` is prepared for execution
- **THEN** the system derives a stable private workspace key from the workflow instance id and logical task id

#### Scenario: Group workspace key is derived
- **WHEN** a task with `workspace.group_name` is prepared for execution
- **THEN** the system derives a stable group workspace key from the workflow instance id and normalized workspace group name

#### Scenario: Private workspace path is created
- **WHEN** a task without `workspace.group_name` is dispatched to an eligible worker
- **THEN** the worker creates or resolves a directory under its worker-local workspace root for the workflow instance and logical task ID

#### Scenario: Group workspace path is created
- **WHEN** a task with `workspace.group_name` is dispatched to an eligible worker
- **THEN** the worker creates or resolves a directory under its worker-local workspace root from the derived group workspace key

#### Scenario: Workspace path is created on pinned host
- **WHEN** a task is dispatched for a workflow instance
- **THEN** all workspace directories for that workflow instance are created or resolved on the pinned host

#### Scenario: Workspace records timestamp marker
- **WHEN** the worker creates or touches a workspace directory
- **THEN** the workspace includes a `.timestamp` marker usable by later stale-directory cleanup

### Requirement: Workspace Manager Lifecycle
The system SHALL provide workspace management components for deriving workspace keys, resolving workflow-instance group workspaces, materializing worker-local workspaces, and cleaning RunHelm-owned workspaces.

#### Scenario: Workspace manager derives task workspace
- **WHEN** a task execution is prepared
- **THEN** workspace management derives the selected workspace key for that task

#### Scenario: Workspace manager resolves group key
- **WHEN** a later task in the same workflow instance declares the same normalized group name
- **THEN** workspace management derives the same group workspace key and resolves the same selected workspace identity

#### Scenario: Workspace manager uses workflow pin
- **WHEN** a workflow instance has a host pin
- **THEN** workspace management uses that host pin for worker-local workspace materialization

#### Scenario: Worker materializes selected workspace
- **WHEN** a task dispatch includes a selected workspace key
- **THEN** the worker creates or resolves the selected workspace directory under its local workspace root

#### Scenario: Workspace manager cleans workspace
- **WHEN** cleanup is requested for a RunHelm-owned workspace
- **THEN** workspace management attempts to remove that workspace directory only when cleanup policy allows it

### Requirement: Workspace TTL Cleanup
The system SHALL support configurable workspace TTL cleanup while preserving workspaces required by non-terminal workflows.

#### Scenario: Expired terminal workspace is cleaned
- **WHEN** a RunHelm-owned workspace is older than the configured TTL
- **AND** the owning workflow instance is terminal
- **THEN** workspace cleanup may remove that workspace

#### Scenario: Active workflow workspace is retained
- **WHEN** a RunHelm-owned workspace belongs to a workflow instance in `Pending`, `Running`, or `InputNeeded`
- **THEN** workspace cleanup MUST NOT remove that workspace only because its timestamp is older than the configured TTL

#### Scenario: Caller-driven cleanup checks eligibility
- **WHEN** workspace TTL cleanup is requested
- **THEN** workspace cleanup checks workflow ownership and timestamp eligibility before removing any workspace
