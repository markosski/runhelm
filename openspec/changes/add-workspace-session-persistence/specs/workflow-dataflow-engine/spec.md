## ADDED Requirements

### Requirement: Human Input Continuation Materialization
The workflow engine SHALL materialize human-input continuations with the same logical task identity and workflow pin needed for dataflow, workspace, and Agent session reuse.

#### Scenario: Human input continuation is materialized
- **WHEN** human input is submitted for a waiting Agent task
- **THEN** the workflow engine materializes or resumes a continuation attempt for the same logical task ID
- **THEN** the continuation attempt carries generation lineage that preserves downstream data binding behavior

#### Scenario: Continuation preserves workflow pin
- **WHEN** a human-input continuation is prepared
- **AND** the workflow instance has a host pin
- **THEN** the workflow engine preserves the workflow instance host pin

## MODIFIED Requirements

### Requirement: Agent Session Key Convention
The orchestrator SHALL provide stable task execution identity that allows workers to derive Agent session keys when Agent execution creates or reuses a durable session.

#### Scenario: Reusable Agent attempt has logical key inputs
- **WHEN** an Agent task attempt has `reuse_session` set to `true`
- **THEN** the worker can derive the session key from the workflow instance ID and logical task ID

#### Scenario: Agent continuation attempt is materialized
- **WHEN** the engine materializes a later attempt for the same logical Agent task due to human input or verifier feedback
- **THEN** the later attempt carries its `generation_index`
- **THEN** the worker can derive the same session key for that logical Agent task when `reuse_session` is true

#### Scenario: Human-input continuation preserves session identity
- **WHEN** the public human-input submission flow resumes an Agent task with `reuse_session` set to `true`
- **THEN** the resumed attempt preserves workflow instance ID and logical task ID for session key derivation

#### Scenario: Agent session reuse is disabled
- **WHEN** an Agent task attempt has `reuse_session` set to `false`
- **THEN** the worker does not load prior logical-task conversation history

#### Scenario: Non-Agent attempt is materialized
- **WHEN** the engine materializes a Function or API call task attempt
- **THEN** the attempt does not require Agent session key metadata
