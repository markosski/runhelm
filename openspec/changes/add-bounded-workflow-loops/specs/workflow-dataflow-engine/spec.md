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

### Requirement: Bounded Backedge Validation
The orchestrator SHALL preserve ordinary data binding cycle validation and SHALL allow bounded verifier backedges only through verifier configuration.

#### Scenario: Ordinary data binding cycle
- **WHEN** workflow data bindings contain a cycle such as `A -> B -> A`
- **THEN** the workflow definition is rejected

#### Scenario: Explicit bounded backedge
- **WHEN** workflow data bindings contain `A -> B -> C -> D` and Agent task `D` declares `on_failure_rerun_task: B`
- **THEN** the workflow definition is accepted as a bounded verifier-controlled rerun

#### Scenario: Backedge target is downstream
- **WHEN** Agent task `B` declares `on_failure_rerun_task: D` for downstream task `D`
- **THEN** the workflow definition is rejected

### Requirement: Task Instance Lifecycle Management
The orchestrator SHALL transition each materialized `TaskInstance` through a strictly defined lifecycle. Non-rerun task instances SHALL use `Pending` -> `Running` -> (`Completed` or `Failed`), while verifier-controlled generations SHALL be selected before downstream tasks after the verifier can run.

#### Scenario: Valid inputs trigger execution
- **WHEN** all input schemas of a `Pending` materialized task instance are satisfied by upstream data bindings
- **THEN** the task status transitions from `Pending` to `Running`

#### Scenario: Workflow initialization without verifier backedges
- **WHEN** a `WorkflowInstance` is initialized from a `WorkflowDef` with no verifier backedges
- **THEN** all tasks are instantiated as `Pending`, and any tasks with no data bindings immediately transition to `Running`

#### Scenario: First bounded generation initialization
- **WHEN** a verifier-controlled rerun slice becomes eligible to run
- **THEN** the first generation of tasks in that slice is materialized

#### Scenario: Bounded generation completion
- **WHEN** a verifier-controlled generation produces schema-valid outputs and its verifier returns `complete`
- **THEN** the generation is selected and downstream tasks after the verifier become eligible

### Requirement: Data Binding Resolution
The orchestrator SHALL construct executable workflow dataflow from the `DataBinding`s in the `WorkflowDef`, resolving bounded rerun source task IDs to the selected generation when a task participates in a verifier-controlled rerun slice.

#### Scenario: Sequential propagation
- **WHEN** Task A completes successfully outside verifier-controlled rerun handling
- **THEN** the output of Task A is mapped to the input payload of Task B according to the defined `DataBinding`

#### Scenario: Fan-In propagation
- **WHEN** Task C requires inputs from both Task A and Task B
- **THEN** Task C SHALL NOT transition to `Running` until both Task A and Task B have successfully completed and populated their respective input bindings on Task C

#### Scenario: Same-generation propagation inside rerun slice
- **WHEN** rerun generation 2 includes `B[2] -> C[2] -> D[2]`
- **THEN** `C[2]` receives output from `B[2]` and `D[2]` receives output from `C[2]`

#### Scenario: Selected generation propagation after verifier
- **WHEN** verifier task `D[2]` is accepted
- **THEN** downstream tasks bound to `D` receive output from `D[2]`

#### Scenario: Rejected generation does not propagate after verifier
- **WHEN** verifier task `D[1]` is rejected and another generation will run
- **THEN** downstream tasks bound after `D` do not receive output from `D[1]`
