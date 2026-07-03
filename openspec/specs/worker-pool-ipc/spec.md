# Capability: worker-pool-ipc

## Purpose
Defines how the orchestrator coordinates long-lived worker processes over Unix Domain Socket IPC for registration, task dispatch, result handling, failure recovery, and timeout management.

## Requirements

### Requirement: Orchestrator IPC Server
The Orchestrator SHALL host a Unix Domain Socket server to receive connections from long-lived worker processes.

#### Scenario: Orchestrator starts IPC server
- **WHEN** the Orchestrator application starts up
- **THEN** it binds to a Unix Domain Socket at the path specified in the configuration (defaulting to `/tmp/runhelm.sock`)

### Requirement: Workflow-Pin Task Claiming
The orchestrator SHALL only allow a worker to claim a task when the worker satisfies the workflow instance's host pin constraint.

#### Scenario: Workflow pin already exists before claim
- **WHEN** a task dispatch belongs to a workflow instance
- **THEN** the dispatch has an existing workflow instance `pinned_host_id` before any worker claims it

#### Scenario: Workflow has required host pin
- **WHEN** a task dispatch belongs to a workflow instance pinned to a specific host ID
- **AND** a worker with a different host ID claims work
- **THEN** the orchestrator does not dispatch that task to that worker

#### Scenario: Matching host claims pinned workflow task
- **WHEN** a task dispatch belongs to a workflow instance pinned to a specific host ID
- **AND** an eligible worker registered with that host ID claims work
- **THEN** the orchestrator may dispatch that task to the worker

#### Scenario: Single active task per workflow instance
- **WHEN** a workflow instance already has an active task dispatch
- **THEN** the orchestrator does not dispatch another task for the same workflow instance until the active dispatch completes, expires, or is released

### Requirement: Worker Connection and Registration
Workers SHALL connect to the Orchestrator's socket and provide a registration message identifying their worker process and configured stable host identity.

#### Scenario: Worker host id is required
- **WHEN** a Worker process starts without `RUNHELM_WORKER_HOST_ID`
- **THEN** the Worker fails startup or registration with a clear host identity configuration error

#### Scenario: Successful worker registration
- **WHEN** a Worker process connects to the socket and sends a valid registration JSON with worker ID and host ID
- **THEN** the Orchestrator adds that connection to its active worker pool and marks it as "Idle"
- **AND** the Orchestrator returns the heartbeat interval the worker must use

#### Scenario: Worker registers stable host identity
- **WHEN** a Worker process registers
- **THEN** the registration identifies the stable host from `RUNHELM_WORKER_HOST_ID` whose local workspace and session stores the worker can access

#### Scenario: Worker registration omits host identity
- **WHEN** a Worker process registers without a host ID
- **THEN** the Orchestrator rejects the registration

### Requirement: Worker Heartbeat Registration
Workers SHALL maintain registration by sending heartbeat messages that join or renew the worker registration.

#### Scenario: Heartbeat joins worker
- **WHEN** a worker sends a heartbeat with valid worker ID and host ID
- **AND** that worker ID is not currently registered
- **THEN** the orchestrator registers the worker as available

#### Scenario: Heartbeat renews worker
- **WHEN** a registered worker sends a heartbeat before its liveness deadline expires
- **THEN** the orchestrator extends that worker registration's liveness deadline

#### Scenario: Missed heartbeat deregisters worker
- **WHEN** a worker misses the configured heartbeat threshold
- **THEN** the orchestrator deregisters that worker ID
- **THEN** the host ID remains a durable placement identity for workflow pins

#### Scenario: Missed heartbeat makes worker ineligible before deregistration
- **WHEN** a registered worker misses one heartbeat deadline
- **THEN** the orchestrator marks that worker registration as having missed a heartbeat
- **THEN** the orchestrator does not assign new task dispatches to that worker
- **AND** the orchestrator keeps the worker registration until the configured missed heartbeat threshold elapses

#### Scenario: Deregistered worker rejoins by heartbeat
- **WHEN** a deregistered worker later sends a valid heartbeat
- **THEN** the orchestrator treats the heartbeat as a fresh join for that worker ID

#### Scenario: Multiple workers share host
- **WHEN** multiple registered workers advertise the same host ID
- **THEN** each worker is eligible to execute tasks for workflow instances pinned to that host ID

### Requirement: Exclusive Task Dispatching
The Orchestrator SHALL dispatch a task to exactly one idle worker from the pool whose registration satisfies the task's workflow pin and capability constraints.

#### Scenario: Dispatching a task to an idle worker
- **WHEN** the Workflow Engine needs to execute a task
- **AND** there is at least one "Idle" worker in the pool that satisfies the task constraints
- **THEN** the Orchestrator sends the task JSON to one matching worker's socket and marks that worker as "Busy"

#### Scenario: No matching worker is available
- **WHEN** the Workflow Engine needs to execute a task
- **AND** no idle worker satisfies the task constraints
- **THEN** the Orchestrator leaves the task undispatched and observable as waiting for eligible capacity

#### Scenario: Pinned workflow host is unavailable
- **WHEN** the Workflow Engine needs to execute a task for a pinned workflow instance
- **AND** no eligible registered worker currently advertises the pinned host ID
- **THEN** the Orchestrator does not dispatch the task to another host

#### Scenario: Pinned workflow host is declared lost
- **WHEN** the pinned host is unavailable past the configured host loss policy
- **THEN** the Orchestrator marks non-terminal workflow instances pinned to that host as failed

### Requirement: Task Completion via Response Queue
Workers SHALL send the task result back to the Orchestrator via the same socket connection.

#### Scenario: Worker returns task result
- **WHEN** a worker finishes a task and writes the result JSON back to the socket
- **THEN** the Orchestrator receives the result, routes it to the correct workflow task, and marks the worker as "Idle" again

### Requirement: Connection Failure Detection
The Orchestrator SHALL detect when a worker connection is closed or lost and update in-memory dispatch lease state for any task owned by that connection.

#### Scenario: Worker disconnects while idle
- **WHEN** an "Idle" worker closes its socket connection
- **THEN** the Orchestrator removes the worker from the pool

#### Scenario: Worker disconnects while busy
- **WHEN** a "Busy" worker closes its socket connection before sending a result
- **THEN** the Orchestrator removes the worker from the pool
- **THEN** the Orchestrator expires or releases the in-memory task dispatch lease according to recovery policy

#### Scenario: Unregistered worker reports task result after orchestrator restart
- **WHEN** the Orchestrator has restarted and has no active registration for a worker ID
- **AND** that worker reports completion for a task it claimed before the restart
- **THEN** the Orchestrator rejects the result as coming from an unregistered worker
- **THEN** the Orchestrator does not advance workflow state from that result
- **THEN** recovery or retry policy handles the corresponding running task attempt

#### Scenario: Re-registered worker reports stale pre-restart dispatch result
- **WHEN** the Orchestrator has restarted with an empty in-memory dispatch lease table
- **AND** a worker re-registers successfully
- **AND** that worker reports completion for a dispatch ID claimed before the restart
- **THEN** the Orchestrator treats the result as late or untracked unless that dispatch ID matches a current active lease
- **THEN** the Orchestrator acknowledges the result without advancing workflow state
- **THEN** the recovered workflow attempt may already have been redispatched

### Requirement: Orchestrator Startup Recovery
The Orchestrator SHALL reconstruct runnable workflow work from durable workflow state upon application startup while treating worker registrations and dispatch leases as empty in-memory state.

#### Scenario: Orchestrator recovers tasks after crash
- **WHEN** the Orchestrator application starts up
- **THEN** it queries durable workflow state for all non-terminal runnable or in-flight work
- **THEN** it restores pending work to its internal dispatch queue with persisted workflow pin constraints

#### Scenario: Orchestrator recovers abandoned running attempts after crash
- **WHEN** the Orchestrator application starts up
- **AND** durable workflow state contains a running task attempt without an in-memory dispatch lease
- **THEN** it requeues or fails the task according to recovery policy
- **THEN** duplicate execution of that logical task attempt is possible until durable lease reattachment is implemented

### Requirement: Task Timeout Management
The Orchestrator SHALL monitor the execution time of tasks assigned to workers and handle timeouts.

#### Scenario: Task exceeds maximum execution time
- **WHEN** a worker does not return a result within the configured task timeout
- **THEN** the Orchestrator marks the task as "FAILED" in the database and ignores any subsequent results from that specific connection
