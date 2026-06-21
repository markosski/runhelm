## ADDED Requirements

### Requirement: Durable Workflow Resume State
The system SHALL persist enough workflow and scheduling state to resume non-terminal workflow instances after orchestrator restart without losing any workflow-instance host pin.

#### Scenario: Pending workflow is recovered after restart
- **WHEN** the orchestrator starts after a previous process exited
- **AND** a workflow instance snapshot is `Pending` or `Running`
- **THEN** the orchestrator reconstructs or reloads runnable workflow work for that instance
- **THEN** any persisted workflow-instance host pin remains in force

#### Scenario: Terminal workflow is not requeued
- **WHEN** the orchestrator starts after a previous process exited
- **AND** a workflow instance snapshot is terminal
- **THEN** the orchestrator does not enqueue new task execution for that instance

### Requirement: Human Input Resume
The system SHALL allow a workflow waiting for human input to resume by committing the submitted input as durable workflow state and continuing execution with the original logical task identity.

#### Scenario: Human input is submitted
- **WHEN** a workflow instance is in `InputNeeded`
- **AND** the user submits input for the waiting task
- **THEN** the system records the submitted input durably
- **THEN** the system makes the workflow eligible to continue from the waiting logical task

#### Scenario: Human input resume preserves workflow pin
- **WHEN** a human-input-created continuation attempt is prepared
- **AND** the workflow instance has an existing host pin
- **THEN** the continuation attempt uses the existing host pin

### Requirement: Dispatch Lease Recovery
The system SHALL track claimed task dispatches with durable lease metadata so abandoned running work can be recovered after orchestrator or worker failure.

#### Scenario: Claimed task lease is active
- **WHEN** a worker claims a task dispatch
- **THEN** the system records the worker ID, host ID, claim time, and lease expiration for that dispatch

#### Scenario: Expired lease is recovered
- **WHEN** startup recovery or lease monitoring finds a claimed dispatch whose lease has expired
- **THEN** the system stops treating the previous worker claim as active
- **THEN** the system requeues or fails the task according to the configured recovery policy

#### Scenario: Late result is ignored
- **WHEN** a worker returns a result for a dispatch whose lease is no longer active
- **THEN** the system ignores the late result for workflow state transition purposes

### Requirement: Pinned Host Loss Handling
The system SHALL mark a pinned workflow instance as failed when the pinned host is lost or unavailable for required execution.

#### Scenario: Pinned host has no workers
- **WHEN** a task is ready to run
- **AND** the workflow instance is pinned to a host with no eligible registered worker
- **THEN** the task is not dispatched to another host silently
- **THEN** the workflow instance is marked `Failed`
- **THEN** the workflow reports that the pinned host is unavailable

#### Scenario: Replacement host is not selected implicitly
- **WHEN** a workflow instance is pinned to a host
- **AND** that host is lost
- **THEN** the system MUST NOT bind the workflow instance to a different host
- **THEN** the user must decide whether to give up on that workflow instance or retry through an explicit future retry flow
