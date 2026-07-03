# Capability: workflow-resume

## Purpose
Defines durable workflow resume, workflow host pin recovery, human-input continuation, dispatch lease recovery, pinned-host loss handling, and pinned workflow retry behavior.

## Requirements

### Requirement: Durable Workflow Resume State
The system SHALL persist enough workflow and scheduling state to resume non-terminal workflow instances after orchestrator restart without losing any workflow-instance host pin.

#### Scenario: Runnable workflow is recovered after restart
- **WHEN** the orchestrator starts after a previous process exited
- **AND** a workflow instance snapshot is `Pending` or `Running`
- **THEN** the orchestrator reconstructs or reloads runnable workflow work for that instance
- **THEN** any persisted workflow-instance host pin remains in force

#### Scenario: Blocked non-terminal workflow is discovered but not auto-requeued
- **WHEN** the orchestrator starts after a previous process exited
- **AND** a workflow instance snapshot is `Paused` or `InputNeeded`
- **THEN** the orchestrator discovers the workflow instance as non-terminal
- **THEN** any persisted workflow-instance host pin remains in force
- **THEN** the orchestrator does not enqueue new task execution for that instance until an explicit resume action occurs

#### Scenario: Terminal workflow is not requeued
- **WHEN** the orchestrator starts after a previous process exited
- **AND** a workflow instance snapshot is terminal
- **THEN** the orchestrator does not enqueue new task execution for that instance

### Requirement: Workflow Engine Pass Scheduling
The system SHALL avoid overlapping engine passes for the same workflow instance.

#### Scenario: Duplicate pending enqueue is ignored
- **WHEN** a workflow instance is already pending in the workflow execution queue
- **AND** the same workflow instance is enqueued again
- **THEN** the queue keeps a single pending entry for that workflow instance

#### Scenario: Active workflow enqueue is deferred
- **WHEN** a workflow instance already has an active engine pass
- **AND** the same workflow instance is enqueued again
- **THEN** the queue records at most one pending entry for a later pass
- **THEN** the queued pass is not dequeued until the active engine pass completes

### Requirement: Workflow Pause and Resume Control
The system SHALL expose API operations to pause and resume workflow execution without losing valid in-flight task results.

#### Scenario: Specific workflow is paused
- **WHEN** a caller pauses a workflow instance whose snapshot is `Pending` or `Running`
- **THEN** the system records the workflow status as `Paused`
- **THEN** the system removes that workflow instance from the pending execution queue
- **THEN** the system does not dispatch additional tasks for that workflow until it is resumed

#### Scenario: Already paused workflow pause is idempotent
- **WHEN** a caller pauses a workflow instance whose snapshot is already `Paused`
- **THEN** the system accepts the request without recording duplicate state changes
- **THEN** the workflow remains `Paused`

#### Scenario: Specific workflow is resumed
- **WHEN** a caller resumes a workflow instance whose snapshot is `Paused`
- **THEN** the system records the workflow status as `Pending`
- **THEN** the system enqueues that workflow instance for execution

#### Scenario: Bulk pause affects active workflows
- **WHEN** a caller requests all active workflows to pause
- **THEN** the system pauses each workflow instance whose snapshot is `Pending` or `Running`
- **THEN** the system removes each paused workflow instance from the pending execution queue
- **THEN** the response identifies the affected workflow instances

#### Scenario: Bulk resume affects paused workflows
- **WHEN** a caller requests all paused workflows to resume
- **THEN** the system resumes each workflow instance whose snapshot is `Paused`
- **THEN** the system enqueues each resumed workflow instance for execution
- **THEN** the response identifies the affected workflow instances

#### Scenario: In-flight non-final task completes after workflow pause
- **WHEN** a task dispatch is still active for a workflow instance
- **AND** the workflow instance is paused before the task result is committed
- **AND** the task result does not complete the workflow
- **THEN** the system records the task result in durable workflow state
- **THEN** the workflow remains `Paused`
- **THEN** the system does not dispatch downstream work for that workflow

#### Scenario: In-flight final task completes after workflow pause
- **WHEN** a task dispatch is still active for a workflow instance
- **AND** the workflow instance is paused before the task result is committed
- **AND** the task result completes the workflow
- **THEN** the system records the task result in durable workflow state
- **THEN** the workflow is marked `Completed`

#### Scenario: In-flight task reaches stronger blocked or terminal state after workflow pause
- **WHEN** a task dispatch is still active for a workflow instance
- **AND** the workflow instance is paused before the task result is committed
- **AND** the task result fails or requests human input
- **THEN** the system records the task result in durable workflow state
- **THEN** the workflow is marked `Failed` or `InputNeeded` according to the task result

### Requirement: Workflow Pin Creation
The system SHALL create a workflow-instance host pin when a workflow instance is created for execution.

#### Scenario: Workflow pin is selected from registered workers
- **WHEN** a queued workflow instance is created
- **AND** at least one eligible worker is registered
- **THEN** the system selects a host ID from the eligible registered workers
- **THEN** the system persists that host ID as the workflow instance pin

#### Scenario: Workflow creation has no eligible host
- **WHEN** a queued workflow instance is created
- **AND** no eligible worker is registered
- **THEN** the system rejects the creation with a capacity-unavailable error
- **THEN** the system does not create an unpinned workflow instance

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
The system SHALL track claimed task dispatches with in-memory worker-pool lease metadata while the orchestrator process is running, and SHALL recover abandoned running work from durable workflow state after orchestrator restart.

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

#### Scenario: Pre-restart worker result is ignored by fresh orchestrator
- **WHEN** the orchestrator restarts after dispatching a task to a worker
- **AND** the restarted orchestrator has no active lease for the pre-restart dispatch ID
- **AND** the pre-restart worker returns a result for that dispatch
- **THEN** the restarted orchestrator treats the result as late or untracked
- **THEN** the result does not advance workflow state

#### Scenario: Restart recovery may redispatch abandoned running attempt
- **WHEN** the orchestrator restarts while a worker is still processing a task
- **AND** durable workflow state still marks the workflow or task attempt as `Running`
- **THEN** startup recovery may make that attempt runnable again according to recovery policy
- **THEN** the same logical task attempt may execute more than once until durable dispatch lease reattachment exists

### Requirement: Pinned Host Loss Handling
The system SHALL mark a pinned workflow instance as failed when the pinned host is declared lost.

#### Scenario: Pinned host temporarily has no workers
- **WHEN** a task is ready to run
- **AND** the workflow instance is pinned to a host with no eligible registered worker
- **THEN** the task is not dispatched to another host silently
- **THEN** the workflow reports that the pinned host is currently unavailable

#### Scenario: Pinned host is declared lost
- **WHEN** the pinned host remains unavailable past the configured host loss policy
- **THEN** the workflow instance is marked `Failed`
- **THEN** the workflow reports that the pinned host is lost

#### Scenario: Replacement host is not selected implicitly
- **WHEN** a workflow instance is pinned to a host
- **AND** that host is lost
- **THEN** the system MUST NOT bind the workflow instance to a different host
- **THEN** the user must decide whether to give up on that workflow instance, retry on the same host, or force retry on a new host

### Requirement: Pinned Workflow Retry
The system SHALL preserve host pins for ordinary retries and SHALL allow explicit force retry to use another eligible host when the existing pinned host is unavailable.

#### Scenario: Default retry uses same host
- **WHEN** a failed pinned workflow instance is retried without force
- **THEN** the retry keeps the existing workflow host pin

#### Scenario: Force retry keeps available existing host
- **WHEN** a failed pinned workflow instance is retried with force
- **AND** the existing pinned host has an eligible registered worker
- **THEN** the retry keeps the existing workflow host pin
- **THEN** the workflow does not record host-local workspace or Agent session context loss

#### Scenario: Force retry reassigns unavailable host
- **WHEN** a failed pinned workflow instance is retried with force
- **AND** the existing pinned host has no eligible registered worker
- **AND** at least one eligible worker is registered
- **THEN** the system assigns a host ID from eligible registered workers
- **THEN** the workflow records that host-local workspace and Agent session context may be lost

#### Scenario: Force retry has no eligible host
- **WHEN** a failed pinned workflow instance is retried with force
- **AND** no eligible worker is registered
- **THEN** the system rejects the retry with a capacity-unavailable error
