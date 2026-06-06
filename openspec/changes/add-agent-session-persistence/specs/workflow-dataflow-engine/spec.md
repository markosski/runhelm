## ADDED Requirements

### Requirement: Agent Session Key Convention
The orchestrator SHALL provide stable task execution identity that allows workers to derive Agent session keys when Agent execution creates or reuses a durable session.

#### Scenario: Reusable Agent attempt has logical key inputs
- **WHEN** an Agent task attempt has `reuse_session` set to `true`
- **THEN** the worker can derive the session key from the workflow instance ID and logical task ID

#### Scenario: Agent continuation attempt is materialized
- **WHEN** the engine materializes a later attempt for the same logical Agent task due to human input or verifier feedback
- **THEN** the later attempt carries its `generation_index`
- **THEN** the worker can derive the same session key for that logical Agent task when `reuse_session` is true

#### Scenario: Human-input continuation API is deferred
- **WHEN** this change prepares session handling for human-input-created Agent attempts
- **THEN** the public human-input submission API and end-to-end resume flow are completed by a separate change
- **THEN** this change still requires future human-input continuation attempts to preserve workflow instance ID and logical task ID for session key derivation

#### Scenario: Agent session reuse is disabled
- **WHEN** an Agent task attempt has `reuse_session` set to `false`
- **THEN** the worker does not load prior logical-task conversation history

#### Scenario: Non-Agent attempt is materialized
- **WHEN** the engine materializes a Function or API call task attempt
- **THEN** the attempt does not require Agent session key metadata

### Requirement: Agent Session Keys Preserve Workflow Determinism
Agent session keys SHALL preserve conversational continuity without replacing structured workflow attempt state.

#### Scenario: Session-backed attempt has lineage
- **WHEN** an Agent task attempt uses a durable session key
- **THEN** the task attempt still records its task status, satisfaction status, generation index, input mapping, and attempt cause metadata

#### Scenario: Downstream bindings resolve outputs
- **WHEN** downstream task input binding resolves an Agent task output
- **THEN** binding resolution uses completed and satisfied task attempts rather than reading the Agent session transcript

#### Scenario: Waiting task reports input needed
- **WHEN** an Agent task attempt is waiting for human input
- **THEN** the workflow state records the `InputNeeded` status and request description independently of the Agent session transcript

### Requirement: Agent Session Recovery Lifecycle
The workflow engine SHALL keep workflow attempt state independent of recoverable Agent session-load problems.

#### Scenario: Worker recovers from session-load problem
- **WHEN** an Agent task attempt cannot load its expected reusable session
- **THEN** the worker logs the session-load problem and continues execution with a fresh session
- **THEN** the workflow task status is determined by the Agent execution result

#### Scenario: Recovered continuation does not bypass validation
- **WHEN** an Agent continuation attempt recovers from a session-load problem
- **THEN** downstream binding satisfaction still depends on the task execution result and normal verifier/dataflow rules
