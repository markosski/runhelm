## ADDED Requirements

### Requirement: Agent Verifier Definition
The system SHALL allow Agent task definitions to declare a verifier block with a positive maximum iteration count, an exhaustion policy, a loop context input index, and inline dependency-free verifier code.

#### Scenario: Agent declares verifier
- **WHEN** an Agent task definition contains a verifier block with `max_iterations`, `on_exhausted`, `loop_context_input_index`, and verifier `code`
- **THEN** the workflow definition is accepted as a verified Agent task definition

#### Scenario: Non-Agent declares verifier
- **WHEN** a non-Agent task definition contains a verifier block
- **THEN** the workflow definition is rejected

#### Scenario: Verifier declares dependencies
- **WHEN** a verifier block declares external dependencies
- **THEN** the workflow definition is rejected

#### Scenario: Agent omits verifier
- **WHEN** an Agent task definition does not contain a verifier block
- **THEN** the Agent task executes with the existing non-verified task behavior

### Requirement: Worker-Side Verifier Execution
The system SHALL execute Agent verifier code in the worker after the Agent attempt output passes task output schema validation.

#### Scenario: Agent output is schema-valid
- **WHEN** a verified Agent attempt produces output that satisfies the Agent task output schema
- **THEN** the worker executes verifier code with the verifier context for that attempt

#### Scenario: Agent output is schema-invalid
- **WHEN** a verified Agent attempt produces output that does not satisfy the Agent task output schema
- **THEN** the verifier code is not executed for that attempt

### Requirement: Verifier Decision Contract
The system SHALL require verifier code to return either `{ "decision": "complete" }` or `{ "decision": "continue", "feedback": "<non-empty string>" }`.

#### Scenario: Verifier accepts attempt
- **WHEN** verifier code returns `decision` equal to `complete`
- **THEN** the system marks the Agent attempt as accepted and allows the task to satisfy downstream bindings

#### Scenario: Verifier rejects attempt with feedback
- **WHEN** verifier code returns `decision` equal to `continue` and the Agent has remaining iterations
- **THEN** the system records verifier feedback and creates the next Agent attempt

#### Scenario: Continue omits feedback
- **WHEN** verifier code returns `decision` equal to `continue` without non-empty `feedback`
- **THEN** the verifier result is invalid and the system marks the workflow as failed

#### Scenario: Verifier output is invalid
- **WHEN** verifier code does not return a valid verifier decision
- **THEN** the system marks the workflow as failed

### Requirement: Verified Agent Attempt Materialization
The system SHALL persist each verified Agent execution as a distinct materialized attempt with a stable attempt ID and the original task definition ID.

#### Scenario: First verified Agent attempt runs
- **WHEN** verified Agent task `implementchange` runs for the first time
- **THEN** the system persists an attempt such as `implementchange[1]` whose task definition ID is `implementchange`

#### Scenario: Later verified Agent attempt runs
- **WHEN** verified Agent task `implementchange` runs for a second time after verifier feedback
- **THEN** the system persists a separate attempt such as `implementchange[2]` without overwriting `implementchange[1]`

#### Scenario: Rejected attempt remains observable
- **WHEN** a verified Agent attempt is rejected by verifier decision `continue`
- **THEN** the system retains the rejected attempt in workflow state without allowing it to satisfy downstream bindings

#### Scenario: Successful rejected attempt remains completed
- **WHEN** a verified Agent attempt produces schema-valid output and verifier decision `continue`
- **THEN** the task attempt lifecycle status is `Completed` and verifier metadata records that the attempt was rejected

### Requirement: Loop Context Injection
The system SHALL provide orchestrator-owned loop context to repeated verified Agent attempts without requiring the workflow trigger payload to include that context.

#### Scenario: Repeated Agent receives feedback
- **WHEN** a verifier returns feedback and creates a next Agent attempt
- **THEN** the next Agent attempt receives loop context containing iteration number, maximum iteration count, latest feedback, and feedback history at the configured input index

#### Scenario: First Agent attempt has no prior feedback
- **WHEN** the first verified Agent attempt runs
- **THEN** the loop context contains no prior feedback

#### Scenario: Verifier context excludes prior verifier result
- **WHEN** verifier code is executed
- **THEN** the verifier context contains `output`, `attempt`, `max_iterations`, and `feedback_history`, and does not contain prior verifier result

### Requirement: Verifier Exhaustion Policy
The system SHALL apply the verified Agent's configured exhaustion policy when the verifier requests continuation after the maximum iteration count has been reached.

#### Scenario: Exhausted verifier fails workflow
- **WHEN** a verified Agent reaches `max_iterations`, the verifier decision is `continue`, and `on_exhausted` is `fail`
- **THEN** the system marks the workflow as failed and records that the Agent exhausted its verifier iteration budget

#### Scenario: Exhausted verifier continues workflow
- **WHEN** a verified Agent reaches `max_iterations`, the verifier decision is `continue`, and `on_exhausted` is `continue`
- **THEN** the system finalizes the highest available Agent attempt and allows it to satisfy downstream bindings

#### Scenario: Exhausted verifier continue has no schema-valid output
- **WHEN** a verified Agent reaches `max_iterations`, `on_exhausted` is `continue`, and there is no schema-valid latest attempt output
- **THEN** the system marks the workflow as failed

### Requirement: Verified Output Binding
The system SHALL satisfy downstream bindings from a verified Agent task only with its accepted or exhaustion-finalized attempt output.

#### Scenario: Agent accepted after first attempt
- **WHEN** a verified Agent attempt `implementchange[1]` is accepted by verifier decision `complete`
- **THEN** downstream tasks bound to `implementchange` receive output from `implementchange[1]`

#### Scenario: Agent accepted after later attempt
- **WHEN** `implementchange[1]` is rejected and `implementchange[2]` is accepted
- **THEN** downstream tasks bound to `implementchange` receive output from `implementchange[2]`

#### Scenario: Agent has no accepted attempt
- **WHEN** a verified Agent has only rejected attempts and its exhaustion policy is `fail`
- **THEN** downstream tasks bound to that Agent do not run

### Requirement: Verified Agent Observability
The system SHALL expose verified Agent attempts and verifier exit state through persisted workflow state and read APIs.

#### Scenario: Non-verified task has no verifier metadata
- **WHEN** a task does not declare a verifier block
- **THEN** the persisted task instance and read API response contain no verifier metadata

#### Scenario: Status includes verified Agent attempts
- **WHEN** a workflow status report is requested for a workflow with verified Agent attempts
- **THEN** the report includes materialized attempts such as `implementchange[1]` and `implementchange[2]`

#### Scenario: Task result lookup uses attempt ID
- **WHEN** a task result is requested for a materialized verified Agent attempt
- **THEN** the system returns the result for that specific attempt ID

### Requirement: Verified Agent Persistence and Logical Lookup
The system SHALL persist verified Agent attempts as task instances keyed by materialized attempt ID and SHALL persist a logical verified Agent state index keyed by the original task ID.

#### Scenario: Attempts are persisted as task instances
- **WHEN** verified Agent task `implementchange` produces two attempts
- **THEN** workflow state contains task instances keyed by `implementchange[1]` and `implementchange[2]`

#### Scenario: Logical verified Agent state tracks accepted attempt
- **WHEN** verified Agent attempt `implementchange[2]` is accepted
- **THEN** workflow state records `implementchange[2]` as the accepted attempt for logical task ID `implementchange`

#### Scenario: Logical task result lookup resolves accepted attempt
- **WHEN** task result lookup requests logical task ID `implementchange`
- **THEN** the system returns the output of the accepted or exhaustion-finalized attempt and includes the resolved attempt ID

#### Scenario: Exact attempt lookup returns historical attempt
- **WHEN** task result lookup requests materialized attempt ID `implementchange[1]`
- **THEN** the system returns the result and verifier metadata for that exact historical attempt

### Requirement: Verified Agent Side Effects
The system SHALL retain rejected Agent attempts as audit history and SHALL NOT automatically roll back side effects produced by rejected attempts.

#### Scenario: Rejected attempt has side effects
- **WHEN** a verified Agent attempt performs side effects and verifier decision is `continue`
- **THEN** the system records the rejected attempt and does not roll back those side effects
