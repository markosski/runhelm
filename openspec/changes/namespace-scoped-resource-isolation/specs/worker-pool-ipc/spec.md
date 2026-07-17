## ADDED Requirements

### Requirement: Namespace-Preserving Worker Round Trip
Worker task claim and result contracts SHALL preserve the namespace assigned by the orchestrator without resolving namespace from worker environment or public request credentials.

#### Scenario: Worker receives task namespace
- **WHEN** a worker claims a task
- **THEN** the task payload identifies the namespace that owns the workflow execution

#### Scenario: Worker returns task namespace
- **WHEN** a worker reports a task result
- **THEN** the result echoes the claimed dispatch namespace
- **AND** the orchestrator validates it against the active dispatch before advancing workflow state

#### Scenario: Worker configuration cannot change task namespace
- **WHEN** worker environment differs from orchestrator namespace configuration
- **THEN** the worker executes and returns the namespace carried by the claimed task
- **AND** it does not derive a replacement namespace from local configuration

