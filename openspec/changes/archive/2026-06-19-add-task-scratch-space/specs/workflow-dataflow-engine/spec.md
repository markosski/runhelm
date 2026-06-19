## ADDED Requirements

### Requirement: Workspace Group Definition Validation
The orchestrator SHALL validate task `workspace.group_name` declarations during workflow registration.

#### Scenario: Valid workspace group is registered
- **WHEN** a task declares `workspace.group_name` using a valid workspace group identifier
- **THEN** the workflow definition is accepted

#### Scenario: Invalid workspace group is rejected
- **WHEN** a task declares `workspace.group_name` using an invalid workspace group identifier
- **THEN** the workflow definition is rejected

#### Scenario: Multiple workspace groups are rejected
- **WHEN** a task definition attempts to declare more than one workspace group
- **THEN** the workflow definition is rejected

### Requirement: Workspace Group Selection
The workflow engine SHALL select either the default private workspace or the declared workspace group for each task.

#### Scenario: Task has no workspace group
- **WHEN** a task definition omits `workspace.group_name`
- **THEN** the workflow engine selects the task's private logical-task workspace

#### Scenario: Task has workspace group
- **WHEN** a task definition declares `workspace.group_name`
- **THEN** the workflow engine selects a group workspace identity derived from the workflow instance id and normalized group name
- **THEN** the task's private logical-task workspace is not selected for that execution

#### Scenario: Same group resolves same workspace identity
- **WHEN** multiple task definitions in a workflow instance declare the same `workspace.group_name`
- **THEN** the workflow engine resolves those tasks to the same group workspace identity

### Requirement: Workspace Groups Do Not Define Scheduling
Workspace group membership SHALL NOT create implicit task dependencies or change data binding scheduling behavior.

#### Scenario: Shared workspace without data dependency
- **WHEN** two tasks declare the same `workspace.group_name` but no data binding or control dependency orders them
- **THEN** the workflow engine does not infer an execution order from the shared workspace group

#### Scenario: Data binding still controls scheduling
- **WHEN** task B depends on task A through a JSON data binding and both tasks declare the same `workspace.group_name`
- **THEN** task B remains ineligible until task A satisfies the normal data binding requirements
