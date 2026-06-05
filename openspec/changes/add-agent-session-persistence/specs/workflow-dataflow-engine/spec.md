## ADDED Requirements

### Requirement: Agent Session Key Metadata
The orchestrator SHALL derive stable Agent session keys for Agent task attempts when Agent execution creates or reuses a durable session.

#### Scenario: Reusable Agent attempt derives a logical-task session key
- **WHEN** an Agent task attempt has `reuse_session` set to `true`
- **THEN** the engine derives the session key from the workflow instance ID and logical task ID

#### Scenario: Agent continuation attempt is materialized
- **WHEN** the engine materializes a later attempt for the same logical Agent task due to human input or verifier feedback
- **THEN** the later attempt uses the same derived session key for that logical Agent task when `reuse_session` is true

#### Scenario: Agent session reuse is disabled
- **WHEN** an Agent task attempt has `reuse_session` set to `false`
- **THEN** the engine does not dispatch a session key that loads prior logical-task conversation history

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

### Requirement: Agent Session Failure Lifecycle
The workflow engine SHALL treat required Agent session load failures as task execution failures.

#### Scenario: Worker reports session-load failure
- **WHEN** an Agent task attempt fails because its required session key could not be loaded
- **THEN** the task attempt transitions to `Failed`
- **THEN** the workflow transitions to `Failed`

#### Scenario: Failed continuation does not satisfy downstream tasks
- **WHEN** an Agent continuation attempt fails due to a session-load failure
- **THEN** the attempt does not satisfy downstream bindings
