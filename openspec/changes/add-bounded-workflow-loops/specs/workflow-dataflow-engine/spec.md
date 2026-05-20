## MODIFIED Requirements

### Requirement: Workflow Identifier Normalization
The orchestrator SHALL normalize workflow IDs and task IDs to lowercase during workflow registration and SHALL reject IDs containing non-alphanumeric characters.

#### Scenario: Uppercase IDs are registered
- **WHEN** a workflow definition contains uppercase workflow or task IDs using only alphanumeric characters
- **THEN** the system registers those IDs in lowercase form

#### Scenario: ID contains brackets
- **WHEN** a workflow or task ID contains `[` or `]`
- **THEN** the system rejects the workflow definition

#### Scenario: ID contains non-alphanumeric character
- **WHEN** a workflow or task ID contains a character other than an ASCII letter or digit
- **THEN** the system rejects the workflow definition

### Requirement: Task Instance Lifecycle Management
The orchestrator SHALL transition each materialized `TaskInstance` through a strictly defined lifecycle. Non-verified task instances SHALL use `Pending` -> `Running` -> (`Completed` or `Failed`), while verified Agent attempts SHALL be verified before their parent task can be considered completed.

#### Scenario: Valid inputs trigger execution
- **WHEN** all `input_schemas` of a `Pending` materialized task instance are satisfied by upstream data bindings
- **THEN** the task status transitions from `Pending` to `Running`

#### Scenario: Workflow initialization
- **WHEN** a `WorkflowInstance` is initialized from a `WorkflowDef` with no verified Agent tasks
- **THEN** all tasks are instantiated as `Pending`, and any tasks with no data bindings immediately transition to `Running`

#### Scenario: Verified Agent attempt initialization
- **WHEN** a `WorkflowInstance` is initialized from a `WorkflowDef` with a verified Agent task
- **THEN** the first Agent attempt is materialized when the Agent task becomes eligible to run

#### Scenario: Verified Agent completion
- **WHEN** a verified Agent attempt produces schema-valid output and its verifier returns `complete`
- **THEN** the Agent task is considered completed using that accepted attempt output

### Requirement: Data Binding Resolution
The orchestrator SHALL construct executable workflow dataflow from the `DataBinding`s in the `WorkflowDef`, resolving verified Agent source task IDs to their highest accepted or exhaustion-finalized attempt.

#### Scenario: Sequential propagation
- **WHEN** Task A completes successfully outside verified Agent attempt handling
- **THEN** the output of Task A is mapped to the input payload of Task B according to the defined `DataBinding`

#### Scenario: Fan-In propagation
- **WHEN** Task C requires inputs from both Task A and Task B
- **THEN** Task C SHALL NOT transition to `Running` until both Task A and Task B have successfully completed and populated their respective input bindings on Task C

#### Scenario: Verified Agent propagation
- **WHEN** a downstream task is bound to verified Agent task `implementchange`
- **THEN** the downstream task receives output from the highest accepted or exhaustion-finalized `implementchange` attempt

#### Scenario: Rejected Agent attempt does not propagate
- **WHEN** verified Agent attempt `implementchange[1]` is rejected and another attempt will run
- **THEN** downstream tasks bound to `implementchange` do not receive output from `implementchange[1]`
