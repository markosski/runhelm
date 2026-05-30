## MODIFIED Requirements

### Requirement: Workflow Definition Identifier Normalization
The orchestrator SHALL normalize workflow definition IDs and task definition IDs to lowercase during workflow registration and SHALL reject those definition IDs when they contain non-alphanumeric characters.

#### Scenario: Uppercase IDs are registered
- **WHEN** a workflow definition contains uppercase workflow definition or task definition IDs using only ASCII alphanumeric characters
- **THEN** the system registers those IDs in lowercase form

#### Scenario: ID contains brackets
- **WHEN** a workflow definition or task definition ID contains `[` or `]`
- **THEN** the system rejects the workflow definition

#### Scenario: ID contains non-alphanumeric character
- **WHEN** a workflow definition or task definition ID contains a character other than an ASCII letter or digit
- **THEN** the system rejects the workflow definition

#### Scenario: Generated task attempt ID contains brackets
- **WHEN** the orchestrator materializes an internal task attempt ID such as `taska[1]`
- **THEN** that generated attempt ID is not subject to workflow definition ID validation

### Requirement: Bounded Backedge Validation
The orchestrator SHALL preserve ordinary data binding cycle validation and SHALL allow bounded verifier backedges only through `control.verifier` configuration.

#### Scenario: Ordinary data binding cycle
- **WHEN** workflow data bindings contain a cycle such as `A -> B -> A`
- **THEN** the workflow definition is rejected

#### Scenario: Explicit bounded backedge
- **WHEN** workflow data bindings contain `A -> B -> C -> D` and task `D` declares `control.verifier.rerun_from_task_id: B`
- **THEN** the workflow definition is accepted as a bounded verifier-controlled rerun

#### Scenario: Verifier self-rerun
- **WHEN** task `D` declares `control.verifier` without `rerun_from_task_id`
- **THEN** the workflow definition is accepted and verifier `continue` reruns only `D`

#### Scenario: Backedge target is downstream
- **WHEN** task `B` declares `control.verifier.rerun_from_task_id: D` for downstream task `D`
- **THEN** the workflow definition is rejected

#### Scenario: Verifier slices overlap
- **WHEN** multiple verifier controls create rerun slices that share any task
- **THEN** the workflow definition is rejected

### Requirement: Task Instance Lifecycle Management
The orchestrator SHALL transition each materialized `TaskInstance` through a lifecycle and SHALL track satisfaction separately from lifecycle completion.

#### Scenario: Valid inputs trigger execution
- **WHEN** all input schemas of a `Pending` materialized task instance are satisfied by upstream data bindings
- **THEN** the task status transitions from `Pending` to `Running`

#### Scenario: Workflow initialization without verifier backedges
- **WHEN** a `WorkflowInstance` is initialized from a `WorkflowDef`
- **THEN** generation-1 task attempts are materialized for the static workflow graph

#### Scenario: Bounded retry generation materialization
- **WHEN** a verifier-controlled generation returns `continue` and has remaining iteration budget
- **THEN** the next generation is materialized only for tasks in the verifier rerun slice

#### Scenario: Bounded generation completion
- **WHEN** a verifier-controlled generation produces schema-valid outputs and its verifier returns `complete`
- **THEN** the generation is marked satisfied and downstream tasks after the verifier become eligible

#### Scenario: Rejected bounded generation
- **WHEN** a verifier-controlled generation produces schema-valid outputs and its verifier returns `continue`
- **THEN** the generation remains lifecycle `Completed` but is marked unsatisfied for downstream binding

### Requirement: Data Binding Resolution
The orchestrator SHALL construct executable workflow dataflow from the `DataBinding`s in the `WorkflowDef`, resolving source task IDs to concrete materialized attempts by generation scope and satisfaction state.

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

#### Scenario: Input mapping records resolved attempts
- **WHEN** a materialized task receives propagated inputs
- **THEN** the task records `input_mapping` for each consumed source task ID and generation
