## ADDED Requirements

### Requirement: Agent Backedge Verifier Definition
The system SHALL allow Agent task definitions to declare a verifier block with `max_iterations`, `on_exhausted_continue`, `on_failure_rerun_task`, and inline dependency-free verifier `code`.

#### Scenario: Agent declares bounded backedge verifier
- **WHEN** an Agent task definition contains a verifier block with positive `max_iterations`, boolean `on_exhausted_continue`, `on_failure_rerun_task`, and verifier `code`
- **THEN** the workflow definition is accepted as a verified backedge control point when the rerun task is a valid upstream ancestor

#### Scenario: Non-Agent declares verifier
- **WHEN** a non-Agent task definition contains a verifier block
- **THEN** the workflow definition is rejected

#### Scenario: Verifier declares dependencies
- **WHEN** a verifier block declares external dependencies
- **THEN** the workflow definition is rejected

#### Scenario: Verifier rerun target is missing
- **WHEN** a verifier block declares `on_failure_rerun_task` for a task ID that does not exist
- **THEN** the workflow definition is rejected

#### Scenario: Verifier rerun target is not upstream
- **WHEN** a verifier block declares `on_failure_rerun_task` for a task that is not an upstream ancestor of the verifier task
- **THEN** the workflow definition is rejected

### Requirement: Worker-Side Verifier Execution
The system SHALL execute Agent verifier code in the worker after the verifier Agent generation output passes task output schema validation.

#### Scenario: Verifier Agent output is schema-valid
- **WHEN** a verifier Agent generation produces output that satisfies the Agent task output schema
- **THEN** the worker executes verifier code with the verifier context for that generation

#### Scenario: Verifier Agent output is schema-invalid
- **WHEN** a verifier Agent generation produces output that does not satisfy the Agent task output schema
- **THEN** verifier code is not executed for that generation

### Requirement: Verifier Decision Contract
The system SHALL require verifier code to return either `{ "decision": "complete" }` or `{ "decision": "continue", "feedback": "<non-empty string>" }`.

#### Scenario: Verifier accepts generation
- **WHEN** verifier code returns `decision` equal to `complete`
- **THEN** the system selects the current generation and allows downstream tasks after the verifier to run

#### Scenario: Verifier requests rerun with feedback
- **WHEN** verifier code returns `decision` equal to `continue` and the generation has remaining iteration budget
- **THEN** the system records verifier feedback and materializes a new generation beginning at `on_failure_rerun_task`

#### Scenario: Continue omits feedback
- **WHEN** verifier code returns `decision` equal to `continue` without non-empty `feedback`
- **THEN** the verifier result is invalid and the system marks the workflow as failed

#### Scenario: Verifier output is invalid
- **WHEN** verifier code does not return a valid verifier decision
- **THEN** the system marks the workflow as failed

### Requirement: Bounded Rerun Slice Materialization
The system SHALL persist each execution of a verifier-controlled rerun slice as distinct materialized task attempts with stable attempt IDs and original task definition IDs.

#### Scenario: First generation runs
- **WHEN** workflow `A -> B -> C -> D` starts and verifier task `D` declares `on_failure_rerun_task: B`
- **THEN** the first generation persists materialized attempts for `B[1]`, `C[1]`, and `D[1]` while preserving `A` outside the rerun slice

#### Scenario: Later generation runs
- **WHEN** verifier task `D[1]` returns `continue`
- **THEN** the system persists a new generation containing `B[2]`, `C[2]`, and `D[2]` without overwriting generation 1

#### Scenario: Rejected generation remains observable
- **WHEN** verifier task `D[1]` rejects generation 1 with decision `continue`
- **THEN** the system retains `B[1]`, `C[1]`, and `D[1]` in workflow state without allowing generation 1 to satisfy downstream bindings after `D`

### Requirement: Loop Context Injection
The system SHALL provide orchestrator-owned loop context to tasks in repeated verifier-controlled generations without requiring workflow trigger payload to include that context.

#### Scenario: Rerun generation receives feedback
- **WHEN** a verifier returns feedback and creates a next generation
- **THEN** tasks in the next generation receive dedicated loop context containing iteration number, maximum iteration count, latest feedback, and feedback history

#### Scenario: First generation has no prior feedback
- **WHEN** the first verifier-controlled generation runs
- **THEN** the loop context contains no prior feedback

#### Scenario: Loop context is not a task input
- **WHEN** a materialized task execution request includes loop context
- **THEN** the loop context is carried as dedicated execution metadata and does not consume a user-declared `input_schemas` slot

### Requirement: Verifier Exhaustion Policy
The system SHALL apply the verifier's configured exhaustion policy when the verifier requests continuation after the maximum iteration count has been reached.

#### Scenario: Exhausted verifier fails workflow
- **WHEN** a verifier reaches `max_iterations`, returns `continue`, and `on_exhausted_continue` is false
- **THEN** the system marks the workflow as failed and records that the verifier exhausted its iteration budget

#### Scenario: Exhausted verifier continues workflow
- **WHEN** a verifier reaches `max_iterations`, returns `continue`, and `on_exhausted_continue` is true
- **THEN** the system finalizes the latest schema-valid generation and allows downstream bindings after the verifier to run

#### Scenario: Exhausted verifier continue has no schema-valid generation
- **WHEN** a verifier reaches `max_iterations`, `on_exhausted_continue` is true, and there is no schema-valid latest generation output
- **THEN** the system marks the workflow as failed

### Requirement: Generation-Scoped Binding Resolution
The system SHALL satisfy bindings inside a verifier-controlled rerun slice using outputs from the same generation and SHALL satisfy downstream bindings after the verifier only with the accepted or exhaustion-finalized generation.

#### Scenario: Sequential rerun propagation
- **WHEN** `D[1]` rejects `B[1] -> C[1] -> D[1]`
- **THEN** `C[2]` receives output from `B[2]` and `D[2]` receives output from `C[2]`

#### Scenario: Inputs outside rerun slice remain selected
- **WHEN** task `A` is outside the rerun slice and `B` depends on `A`
- **THEN** each generation of `B` receives the selected output from `A`

#### Scenario: Accepted generation propagates downstream
- **WHEN** verifier task `D[2]` returns `complete`
- **THEN** downstream tasks bound to `D` receive output from `D[2]`

#### Scenario: Rejected generation does not propagate downstream
- **WHEN** verifier task `D[1]` returns `continue`
- **THEN** downstream tasks after `D` do not receive output from `D[1]`

### Requirement: Verified Backedge Observability
The system SHALL expose verifier-controlled materialized generations and verifier exit state through persisted workflow state and read APIs.

#### Scenario: Non-verified task has no verifier metadata
- **WHEN** a task does not declare a verifier block
- **THEN** the persisted task instance and read API response contain no verifier metadata

#### Scenario: Status includes materialized generation attempts
- **WHEN** a workflow status report is requested for a workflow with verifier-controlled reruns
- **THEN** the report includes materialized attempts such as `B[1]`, `C[1]`, `D[1]`, `B[2]`, `C[2]`, and `D[2]`

#### Scenario: Task result lookup uses attempt ID
- **WHEN** a task result is requested for a materialized generation attempt
- **THEN** the system returns the result for that specific attempt ID

#### Scenario: Logical task result lookup resolves selected generation
- **WHEN** task result lookup requests logical task ID `D`
- **THEN** the system returns the output of the accepted or exhaustion-finalized `D` attempt and includes the resolved attempt ID

### Requirement: Verified Rerun Side Effects
The system SHALL retain rejected generations as audit history and SHALL NOT automatically roll back side effects produced by rejected generations.

#### Scenario: Rejected generation has side effects
- **WHEN** a task in a rejected generation performs side effects and the verifier decision is `continue`
- **THEN** the system records the rejected generation and does not roll back those side effects
