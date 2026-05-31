## ADDED Requirements

### Requirement: Agent Ask Control Configuration
The orchestrator SHALL configure human-input ask behavior through `control.ask` and SHALL allow `control.ask` only on Agent tasks.

#### Scenario: Agent task declares ask control
- **WHEN** a workflow definition contains an Agent task with `control.ask.max_attempts` greater than zero
- **THEN** the workflow definition is accepted as ask-enabled for that task

#### Scenario: Ask control has no attempts
- **WHEN** a workflow definition contains an Agent task with `control.ask.max_attempts` equal to zero
- **THEN** the workflow definition is rejected

#### Scenario: Function task declares ask control
- **WHEN** a workflow definition contains a Function task with `control.ask`
- **THEN** the workflow definition is rejected

#### Scenario: API task declares ask control
- **WHEN** a workflow definition contains an API call task with `control.ask`
- **THEN** the workflow definition is rejected

#### Scenario: Agent kind declares legacy ask flag
- **WHEN** a workflow definition contains an Agent task with ask configured inside the Agent task kind
- **THEN** the workflow definition is rejected because ask MUST be configured through `control.ask`

### Requirement: Ask-Gated InputNeeded Results
The orchestrator SHALL treat `ExecutionResult::InputNeeded` as valid only for tasks that declare `control.ask`.

#### Scenario: Ask-enabled task requests input
- **WHEN** an ask-enabled Agent task attempt returns `InputNeeded` with a description
- **THEN** the task attempt status becomes `InputNeeded` with that description
- **THEN** the workflow status becomes `InputNeeded`

#### Scenario: Non-ask task requests input
- **WHEN** a task attempt without `control.ask` returns `InputNeeded`
- **THEN** the task attempt fails with a clear error
- **THEN** the workflow fails

### Requirement: Human Input Attempt Materialization
The orchestrator SHALL preserve the original `InputNeeded` task attempt and SHALL create a later attempt of the same logical task when human input is provided within the task's finite ask budget.

#### Scenario: Human input is submitted for a waiting task
- **WHEN** task `B[1]` is in `InputNeeded` and human input is submitted for `B[1]`
- **THEN** the orchestrator creates a new attempt `B[2]`
- **THEN** `B[1]` remains in `InputNeeded`
- **THEN** `B[2]` records lineage to `B[1]` and carries the human response as execution context

#### Scenario: Human input attempt completes
- **WHEN** a human-input-created task attempt completes successfully
- **THEN** that task attempt becomes eligible to satisfy downstream bindings
- **THEN** downstream tasks consume the completed human-input attempt rather than the prior `InputNeeded` attempt

#### Scenario: InputNeeded attempt has no output
- **WHEN** a task attempt is in `InputNeeded`
- **THEN** that task attempt MUST NOT expose a task output for downstream data binding
- **THEN** that task attempt MUST NOT satisfy downstream bindings

#### Scenario: Ask attempt budget remains
- **WHEN** an ask-enabled task has used fewer than `control.ask.max_attempts` human-input-created attempts
- **THEN** submitted human input creates the next local attempt for that task

#### Scenario: Ask attempt budget is exhausted
- **WHEN** an ask-enabled task is in `InputNeeded` and submitting human input would exceed `control.ask.max_attempts`
- **THEN** the workflow fails with a clear ask-attempt-budget-exhausted reason
- **THEN** no additional task attempt is created

### Requirement: Ask Attempt Lineage Metadata
The orchestrator SHALL expose metadata that distinguishes human-input-created attempts from verifier-feedback-created attempts and SHALL provide ordered ask history to ask-created attempts.

#### Scenario: Human-input-created attempt is reported
- **WHEN** a task result or workflow status report includes a task attempt created from human input
- **THEN** the report includes metadata identifying the attempt cause as human input
- **THEN** the report includes the previous task attempt ID
- **THEN** the report includes the ask attempt number and maximum ask attempts

#### Scenario: Waiting task is reported
- **WHEN** a task result or workflow status report includes a task attempt in `InputNeeded`
- **THEN** the report exposes the `InputNeeded` status distinctly from `Running`
- **THEN** the report includes the input request description

#### Scenario: Ask history is provided to later attempts
- **WHEN** an ask-created attempt is executed after one or more human responses have been submitted
- **THEN** execution metadata includes ordered ask history containing each prior input request description and human response for that logical task

### Requirement: Human Input Submission Interface
The orchestrator SHALL provide WorkflowService and HTTP API capabilities for submitting human input to unblock an `InputNeeded` task attempt.

#### Scenario: Human input is submitted through WorkflowService
- **WHEN** a caller submits human input to WorkflowService for a workflow instance and task attempt in `InputNeeded`
- **THEN** WorkflowService records the human response in ask history
- **THEN** WorkflowService materializes the next task attempt when ask budget remains
- **THEN** WorkflowService makes the workflow eligible to continue execution

#### Scenario: Human input is submitted through API
- **WHEN** an API caller submits human input for a workflow instance and task attempt in `InputNeeded`
- **THEN** the API delegates to WorkflowService
- **THEN** the API response identifies the newly materialized task attempt

#### Scenario: Human input targets a task that is not waiting
- **WHEN** a caller submits human input for a task attempt that is not in `InputNeeded`
- **THEN** WorkflowService rejects the submission with a clear error

#### Scenario: Human input targets an unknown attempt
- **WHEN** a caller submits human input for an unknown workflow instance or task attempt
- **THEN** WorkflowService rejects the submission with a not-found error

### Requirement: Ask And Verifier Attempt Semantics
The orchestrator SHALL treat ask and verifier feedback as distinct causes for later task attempts while preserving exact source attempt lineage.

#### Scenario: Ask occurs outside verifier rerun handling
- **WHEN** task `B[1]` returns `InputNeeded` outside a verifier rerun slice and human input creates `B[2]`
- **THEN** downstream tasks bound to `B` consume `B[2]` after it completes successfully

#### Scenario: Ask attempt is incomplete rather than rejected
- **WHEN** a task attempt returns `InputNeeded`
- **THEN** the attempt is treated as incomplete and waiting for human input
- **THEN** the attempt is not treated as a verifier-rejected unsatisfied output

#### Scenario: Verifier feedback creates a later attempt
- **WHEN** a verifier returns `continue` for a completed generation
- **THEN** the later attempt records verifier feedback as its cause rather than human input

#### Scenario: Source attempt lineage is exact
- **WHEN** a task receives propagated inputs after ask-created or verifier-created attempts exist
- **THEN** the task records `input_mapping` for the exact source attempts it consumed

## MODIFIED Requirements

### Requirement: Data Binding Resolution
The orchestrator SHALL construct executable workflow dataflow from the `DataBinding`s in the `WorkflowDef`, resolving source task IDs to concrete materialized attempts by verifier slice scope, completion state, and satisfaction state.

#### Scenario: Sequential propagation
- **WHEN** Task A completes successfully outside verifier-controlled rerun handling
- **THEN** the output of Task A is mapped to the input payload of Task B according to the defined `DataBinding`

#### Scenario: Fan-In propagation
- **WHEN** Task C requires inputs from both Task A and Task B
- **THEN** Task C SHALL NOT transition to `Running` until both Task A and Task B have successfully completed and populated their respective input bindings on Task C

#### Scenario: Latest completed propagation inside rerun slice
- **WHEN** a verifier rerun slice contains source task `B` and verifier task `V`
- **THEN** `V` consumes the latest completed attempt for `B` at or after the verifier state's current iteration even when that attempt has a different generation number than `V`

#### Scenario: Verifier waits for current iteration source
- **WHEN** verifier task `V[2]` is pending and source task `B[2]` has not completed
- **THEN** `V[2]` does not consume rejected source attempt `B[1]`

#### Scenario: Selected generation propagation after verifier
- **WHEN** verifier task `D[2]` is accepted
- **THEN** downstream tasks bound to `D` receive output from `D[2]`

#### Scenario: Rejected generation does not propagate after verifier
- **WHEN** verifier task `D[1]` is rejected and another generation will run
- **THEN** downstream tasks bound after `D` do not receive output from `D[1]`

#### Scenario: Input mapping records resolved attempts
- **WHEN** a materialized task receives propagated inputs
- **THEN** the task records `input_mapping` for each consumed source task ID and generation
