# Capability: agent-session-persistence

## Purpose
Defines durable Agent session storage and continuation semantics for Agent task execution.

## Requirements

### Requirement: Durable Agent Session Store
The system SHALL provide a durable Agent session store that can create, load, and persist Agent conversation sessions using stable RunHelm session keys.

#### Scenario: File-backed session is created
- **WHEN** an initial reusable Agent task attempt starts and no session exists for its derived session key
- **THEN** the worker creates a durable session in the configured session store
- **THEN** the session is addressable through the derived session key

#### Scenario: Existing session is loaded
- **WHEN** an Agent continuation attempt starts and a session exists for the derived session key
- **THEN** the worker loads the keyed session from the configured session store before prompting the Agent

#### Scenario: Session key is storage-independent
- **WHEN** the worker derives a session key from workflow instance ID and logical task ID
- **THEN** the key identifies the session through the RunHelm session store boundary rather than requiring a worker-local filesystem path

### Requirement: Agent Session Workflow Pinning
The system SHALL preserve host-local Agent session continuity by using the workflow instance host pin when the configured session store is host-local.

#### Scenario: Agent session uses workflow pin
- **WHEN** an initial reusable Agent task attempt starts in a pinned workflow instance
- **THEN** the Agent session is created or loaded on the pinned host

#### Scenario: Continuation uses workflow pin
- **WHEN** an Agent continuation attempt has `reuse_session` set to `true`
- **AND** the workflow instance has a host pin
- **THEN** the continuation attempt is dispatched only to workers registered for the pinned host

#### Scenario: Blob-backed session store avoids workflow pin dependence
- **WHEN** a worker is configured with a non-host-local session store that can resolve sessions by key from any host
- **THEN** Agent session reuse does not require an additional host-local placement constraint beyond the workflow instance pin

### Requirement: Agent Session Reuse Policy
Agent task definitions SHALL support `reuse_session`, defaulting to `true`.

#### Scenario: Reuse session defaults to true
- **WHEN** an Agent task definition omits `reuse_session`
- **THEN** RunHelm treats `reuse_session` as `true`

#### Scenario: Reusable Agent session key is logical-task scoped
- **WHEN** an Agent task has `reuse_session` set to `true`
- **THEN** RunHelm derives the session key from the workflow instance ID and logical task ID
- **THEN** all materialized attempts for that logical Agent task use the same session key

#### Scenario: Agent session reuse is disabled
- **WHEN** an Agent task has `reuse_session` set to `false`
- **THEN** later attempts for that task MUST NOT load prior conversation history from the logical-task session

### Requirement: Agent Session Continuation
Agent retry and resume attempts SHALL continue the convention-derived Agent session when `reuse_session` is true instead of reconstructing complete conversational history in the orchestrator.

#### Scenario: Initial Agent attempt starts a session
- **WHEN** an initial reusable Agent task attempt starts
- **THEN** the worker prompts the Agent with the task prompt and resolved upstream inputs
- **THEN** the resulting session contains the initial task context

#### Scenario: Human-input attempt continues a session
- **WHEN** a human-input-created Agent attempt has `reuse_session` set to `true`
- **THEN** the worker loads the session identified by the workflow instance ID and logical task ID when it exists
- **THEN** the worker prompts the Agent with the submitted human response as the next session event

#### Scenario: Verifier-feedback attempt continues a session
- **WHEN** a verifier-feedback-created Agent attempt has `reuse_session` set to `true`
- **THEN** the worker loads the session identified by the workflow instance ID and logical task ID when it exists
- **THEN** the worker prompts the Agent with the verifier feedback as the next session event

#### Scenario: Human-input continuation recovers without a session
- **WHEN** a human-input-created Agent attempt has `reuse_session` set to `true`
- **AND** the worker cannot load the session identified by the workflow instance ID and logical task ID
- **THEN** the worker creates a fresh replacement session
- **THEN** the worker prompts the Agent with the task prompt, resolved upstream inputs, and submitted human response

#### Scenario: Verifier-feedback continuation recovers without a session
- **WHEN** a verifier-feedback-created Agent attempt has `reuse_session` set to `true`
- **AND** the worker cannot load the session identified by the workflow instance ID and logical task ID
- **THEN** the worker creates a fresh replacement session
- **THEN** the worker prompts the Agent with the task prompt, resolved inputs or previous output, and verifier feedback

#### Scenario: Full history is not reinjected
- **WHEN** an Agent continuation attempt is executed with a loaded session
- **THEN** the orchestrator does not need to inject complete prior ask or verifier feedback history into the prompt

### Requirement: Missing Session Recovery
The system SHALL log clear diagnostics and recover with a fresh session when a reusable durable session cannot be loaded.

#### Scenario: Expected session is missing
- **WHEN** a reusable Agent continuation attempt references a derived session key that the worker cannot load
- **THEN** the worker logs a clear session-load diagnostic
- **THEN** the worker creates a fresh replacement session and continues execution
- **THEN** the worker includes the full task prompt and current attempt event in the replacement session prompt

#### Scenario: Missing session is observable
- **WHEN** a session-load diagnostic is reported
- **THEN** the diagnostic identifies the session key that could not be loaded

### Requirement: Agent Session Pin Failure Diagnostics
The system SHALL report clear diagnostics when a reusable Agent continuation cannot run because the workflow instance's pinned host is unavailable.

#### Scenario: Pinned session host declared lost
- **WHEN** an Agent continuation attempt belongs to a workflow instance pinned to a host with no eligible registered worker
- **AND** the pinned host is declared lost by host loss policy
- **THEN** the workflow instance is marked `Failed`
- **THEN** the workflow reports that the pinned Agent session host is unavailable

#### Scenario: Session placement diagnostic hides transcript
- **WHEN** the workflow reports an Agent session placement problem
- **THEN** the diagnostic may identify the session key and pinned host ID
- **THEN** the diagnostic MUST NOT include the Agent session transcript

### Requirement: Session Store Extensibility
The Agent session store SHALL support a file-backed implementation initially and SHALL allow future storage implementations without changing workflow attempt semantics.

#### Scenario: Local file-backed storage is configured
- **WHEN** the worker is configured with a local file-backed session store
- **THEN** Agent sessions are persisted under the configured storage location

#### Scenario: Future blob-backed storage is configured
- **WHEN** a future worker is configured with a blob-backed session store
- **THEN** the worker can resolve the same stored session key through the session store boundary
- **THEN** the orchestrator task attempt model does not need to change

### Requirement: Session Privacy Boundary
The system SHALL keep durable session contents out of broad workflow status and result APIs unless a future explicit session inspection capability is added.

#### Scenario: Workflow status reports an Agent attempt
- **WHEN** workflow status includes an Agent task attempt with a session
- **THEN** the status may include the stable session key
- **THEN** the status MUST NOT include the session message transcript
