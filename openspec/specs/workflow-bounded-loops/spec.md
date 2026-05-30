# Capability: workflow-bounded-loops

## Purpose
Defines verifier-controlled bounded workflow reruns, including verifier decisions, generation materialization, loop context, exhaustion behavior, observability, and side-effect expectations.

## Requirements

### Requirement: Verifier Control Definition
The system SHALL allow task definitions to declare `control.verifier` with `max_iterations`, `on_exhausted_continue`, and optional `rerun_from_task_id`.

#### Scenario: Task declares bounded verifier control
- **WHEN** a task definition contains `control.verifier` with positive `max_iterations`, boolean `on_exhausted_continue`, and valid optional `rerun_from_task_id`
- **THEN** the workflow definition is accepted as a verifier-controlled bounded rerun when the rerun task is the verifier itself or a valid upstream ancestor

#### Scenario: Function declares verifier control
- **WHEN** a Function task definition contains `control.verifier` and omits `output_schema`
- **THEN** the workflow definition is accepted and registration injects the verifier decision `output_schema`

#### Scenario: Verifier task declares output schema
- **WHEN** a task definition contains `control.verifier` and also declares `output_schema`
- **THEN** the workflow definition is rejected because RunHelm owns the verifier decision schema

#### Scenario: Verifier decision schema is injected
- **WHEN** a task definition contains `control.verifier` and omits `output_schema`
- **THEN** workflow registration injects the standard verifier decision `output_schema`

#### Scenario: Verifier rerun target is missing
- **WHEN** `control.verifier.rerun_from_task_id` references a task ID that does not exist
- **THEN** the workflow definition is rejected

#### Scenario: Verifier rerun target is not upstream
- **WHEN** `control.verifier.rerun_from_task_id` references a task that is neither the verifier task nor an upstream ancestor of the verifier task
- **THEN** the workflow definition is rejected

#### Scenario: Verifier rerun target is omitted
- **WHEN** `control.verifier.rerun_from_task_id` is omitted
- **THEN** the verifier rerun slice contains only the verifier task

### Requirement: Verifier Decision Contract
The system SHALL treat schema-valid verifier task output as the verifier decision and SHALL require either `{ "decision": "complete" }` or `{ "decision": "continue", "feedback": "<non-empty string>" }`.

#### Scenario: Verifier does not emit corrected business data
- **WHEN** a verifier task runs
- **THEN** its output is interpreted as the verifier control decision rather than as transformed domain output

#### Scenario: Verifier accepts generation
- **WHEN** a verifier task output contains `decision` equal to `complete`
- **THEN** the system selects the current generation and allows downstream tasks after the verifier to run

#### Scenario: Verifier requests rerun with feedback
- **WHEN** a verifier task output contains `decision` equal to `continue` and the generation has remaining iteration budget
- **THEN** the system records verifier feedback and materializes a new generation beginning at `rerun_from_task_id`, or at the verifier task when no rerun target is configured

#### Scenario: Continue omits feedback
- **WHEN** a verifier task output contains `decision` equal to `continue` without non-empty `feedback`
- **THEN** the verifier result is invalid and the system marks the workflow as failed

#### Scenario: Verifier output is invalid
- **WHEN** verifier task output does not satisfy the verifier decision contract
- **THEN** the system marks the workflow as failed

### Requirement: Bounded Rerun Slice Materialization
The system SHALL persist each execution of a verifier-controlled rerun slice as distinct materialized task attempts with stable attempt IDs and original task definition IDs.

#### Scenario: First generation runs
- **WHEN** workflow `A -> B -> C -> D` starts and verifier task `D` declares `rerun_from_task_id: B`
- **THEN** the static workflow attempts include `A[1]`, `B[1]`, `C[1]`, and `D[1]`

#### Scenario: Later generation runs
- **WHEN** verifier task `D[1]` returns `continue`
- **THEN** the system persists a new rerun-slice generation containing `B[2]`, `C[2]`, and `D[2]` without overwriting generation 1

#### Scenario: Rejected generation remains observable
- **WHEN** verifier task `D[1]` rejects generation 1 with decision `continue`
- **THEN** the system retains `B[1]`, `C[1]`, and `D[1]` in workflow state as completed but unsatisfied attempts

### Requirement: Loop Context Injection
The system SHALL provide orchestrator-owned loop context to tasks in repeated verifier-controlled generations without requiring workflow trigger payload to include that context.

#### Scenario: Rerun generation receives feedback
- **WHEN** a verifier returns feedback and creates a next generation
- **THEN** tasks in the next generation receive dedicated loop context containing generation number, maximum iteration count, ordered feedback history, and previous same-task output when present

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

#### Scenario: Input lineage records concrete source attempts
- **WHEN** a task consumes an upstream output
- **THEN** the task attempt records `input_mapping` entries containing the source task ID and source generation consumed

### Requirement: Verified Backedge Observability
The system SHALL expose verifier-controlled materialized generations and verifier exit state through persisted workflow state and read APIs.

#### Scenario: Non-verified task has no verifier metadata
- **WHEN** a task does not declare `control.verifier`
- **THEN** the persisted task instance and read API response contain no verifier metadata

#### Scenario: Status includes materialized generation attempts
- **WHEN** a workflow status report is requested for a workflow with verifier-controlled reruns
- **THEN** the report includes materialized attempts such as `B[1]`, `C[1]`, `D[1]`, `B[2]`, `C[2]`, and `D[2]`

#### Scenario: Status includes verifier summary state
- **WHEN** a workflow status report is requested for a workflow with verifier-controlled reruns
- **THEN** the report includes verifier state sufficient to distinguish running, accepted, exhausted-accepted, exhausted-failed, and failed verifier flows

#### Scenario: Task result lookup uses materialized attempt
- **WHEN** a task result is requested for a materialized generation attempt
- **THEN** the system returns the result for that specific attempt and includes metadata for the resolved attempt

#### Scenario: Logical task result lookup resolves selected generation
- **WHEN** task result lookup requests a logical task ID that has multiple materialized attempts
- **THEN** the system returns the latest selected or satisfied attempt and includes the resolved attempt ID

### Requirement: Verified Rerun Side Effects
The system SHALL retain rejected generations as audit history and SHALL NOT automatically roll back side effects produced by rejected generations.

#### Scenario: Rejected generation has side effects
- **WHEN** a task in a rejected generation performs side effects and the verifier decision is `continue`
- **THEN** the system records the rejected generation and does not roll back those side effects
