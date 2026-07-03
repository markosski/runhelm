## ADDED Requirements

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
