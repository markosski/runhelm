# Capability: task-dispatcher

## Purpose
Defines the behavior of `TaskDispatcher`, the in-memory component that implements `TaskDispatchPort` and coordinates worker task claims, in-flight leases, result waiters, dispatch IDs, task availability notifications, and claimed-task timeouts.

## Requirements

### Requirement: Task Dispatch Enqueue
`TaskDispatcher` SHALL enqueue task dispatches and wait for worker results rather than executing task code directly.

#### Scenario: Task is enqueued
- **WHEN** `TaskDispatcher::dispatch_task` is called with a workflow instance ID, task definition, resolved inputs, execution metadata, and dispatch constraints
- **THEN** it SHALL enqueue a worker-facing task dispatch
- **AND** the enqueued dispatch SHALL include the workflow instance ID, task definition, resolved inputs, execution metadata, workspace path suffix, and dispatch constraints

#### Scenario: No worker claims task
- **WHEN** a task is enqueued and no eligible worker claims it
- **THEN** `TaskDispatcher::dispatch_task` SHALL continue waiting for a worker result until the surrounding caller cancels the future or the pending dispatch is otherwise completed

### Requirement: Dispatch IDs
`TaskDispatcher` SHALL assign worker-facing dispatch IDs that are unique within a process and namespaced across dispatcher instances.

#### Scenario: Fresh dispatcher after restart
- **WHEN** a new `TaskDispatcher` instance dispatches the same logical workflow task as an abandoned earlier dispatcher
- **THEN** the new worker-facing dispatch ID SHALL differ from the earlier dispatch ID

#### Scenario: Stale result
- **WHEN** a result arrives for an unknown or no-longer-active dispatch ID
- **THEN** the dispatcher SHALL acknowledge it as late or untracked without completing any active dispatch waiter

### Requirement: Worker Claim
`TaskDispatcher` SHALL allow workers to claim eligible pending tasks in FIFO order, while skipping pending tasks that are not eligible for the claiming worker.

#### Scenario: Matching worker claims task
- **WHEN** a worker identity matches a pending dispatch's constraints
- **THEN** the worker can claim the dispatch

#### Scenario: Pinned host mismatch
- **WHEN** a pending dispatch is pinned to a different worker host
- **THEN** the worker SHALL NOT claim that dispatch

#### Scenario: Scan past nonmatching task
- **WHEN** the earliest pending dispatch does not match the worker but a later pending dispatch does
- **THEN** the worker SHALL claim the later matching dispatch

#### Scenario: No eligible task
- **WHEN** no pending dispatch is eligible before the worker claim wait timeout elapses
- **THEN** the dispatcher SHALL return no task

### Requirement: Active Lease Limits
`TaskDispatcher` SHALL prevent a worker from holding more than one active dispatch lease and SHALL prevent more than one active dispatch per workflow instance.

#### Scenario: Worker already has active lease
- **WHEN** a worker with an active lease attempts to claim another task
- **THEN** the dispatcher SHALL reject the claim

#### Scenario: Workflow already has active dispatch
- **WHEN** a workflow instance already has an active dispatch lease
- **THEN** other pending dispatches for that same workflow instance SHALL remain pending until the active lease completes

#### Scenario: Multiple workers on same host
- **WHEN** multiple worker identities share the same host and separate workflow instances are pinned to that host
- **THEN** those workers MAY claim separate dispatches concurrently

### Requirement: Result Completion
`TaskDispatcher` SHALL complete the workflow-side waiter associated with a dispatch when a result is reported for the active dispatch ID.

#### Scenario: Worker succeeds
- **WHEN** a worker completes the dispatch with `WorkerExecutionResult::Success`
- **THEN** `TaskDispatcher::dispatch_task` SHALL return `ExecutionResult::Success` with the worker output

#### Scenario: Worker requests input
- **WHEN** a worker completes the dispatch with `WorkerExecutionResult::InputNeeded`
- **THEN** `TaskDispatcher::dispatch_task` SHALL return `ExecutionResult::InputNeeded` with the worker description

#### Scenario: Worker fails
- **WHEN** a worker completes the dispatch with `WorkerExecutionResult::Failure`
- **THEN** `TaskDispatcher::dispatch_task` SHALL return `ExecutionResult::Failure` with the worker reason

### Requirement: Task Timeout
`TaskDispatcher` SHALL track task timeout only after a worker claims the task.

#### Scenario: Task-specific timeout
- **WHEN** a task defines `timeout_secs`
- **THEN** the dispatcher SHALL use that value as the claimed-task timeout

#### Scenario: Queued task waits without timeout
- **WHEN** a task remains pending longer than its dispatch timeout before any worker claims it
- **THEN** the task SHALL remain pending

#### Scenario: Claimed task times out
- **WHEN** a claimed task exceeds its timeout before a worker reports a result
- **THEN** the dispatcher SHALL complete the result waiter with `ExecutionResult::Failure`
- **AND** the active dispatch lease SHALL be removed

#### Scenario: Built-in default timeout
- **WHEN** a task dispatch does not override the timeout and `RUNHELM_TASK_TIMEOUT_SECS` is unset or invalid
- **THEN** the dispatcher SHALL use a 300 second claimed-task timeout

#### Scenario: Environment default timeout
- **WHEN** a task dispatch does not override the timeout and `RUNHELM_TASK_TIMEOUT_SECS` is a valid unsigned integer
- **THEN** the dispatcher SHALL use that value in seconds as the claimed-task timeout

### Requirement: Workspace Dispatch Metadata
`TaskDispatcher` SHALL derive and expose the selected logical workspace path suffix for each task dispatch.

#### Scenario: Private workspace task
- **WHEN** a task does not declare a workspace group
- **THEN** the worker dispatch SHALL identify the private task workspace path suffix for the workflow instance and task ID

#### Scenario: Group workspace task
- **WHEN** a task declares a workspace group
- **THEN** the worker dispatch SHALL identify the group workspace path suffix for the workflow instance and group name

### Requirement: Lost Host Pending Cancellation
`TaskDispatcher` SHALL cancel pending dispatches pinned to hosts that the worker registry has declared lost.

#### Scenario: Pending dispatch pinned to lost host
- **WHEN** the registry reports a lost host
- **AND** a pending dispatch is pinned to that host
- **THEN** the pending dispatch SHALL be cancelled before claim

#### Scenario: Pending dispatch pinned to available host
- **WHEN** the registry reports a different lost host
- **THEN** the pending dispatch SHALL remain pending
