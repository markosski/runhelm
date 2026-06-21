## ADDED Requirements

### Requirement: Workflow-Pin Task Claiming
The orchestrator SHALL only allow a worker to claim a task when the worker satisfies the workflow instance's host pin constraint.

#### Scenario: Workflow pin is established on first claim
- **WHEN** a task dispatch belongs to a workflow instance with no host pin
- **AND** an otherwise eligible idle worker claims work
- **THEN** the orchestrator persists that worker's host ID as the workflow instance host pin
- **THEN** the orchestrator may dispatch that task to the worker

#### Scenario: Workflow has required host pin
- **WHEN** a task dispatch belongs to a workflow instance pinned to a specific host ID
- **AND** a worker with a different host ID claims work
- **THEN** the orchestrator does not dispatch that task to that worker

#### Scenario: Matching host claims pinned workflow task
- **WHEN** a task dispatch belongs to a workflow instance pinned to a specific host ID
- **AND** an eligible worker registered with that host ID claims work
- **THEN** the orchestrator may dispatch that task to the worker

## MODIFIED Requirements

### Requirement: Worker Connection and Registration
Workers SHALL connect to the Orchestrator's socket and provide a registration message identifying their worker process, stable host identity, capabilities, and scheduling labels.

#### Scenario: Successful worker registration
- **WHEN** a Worker process connects to the socket and sends a valid registration JSON with worker ID and host ID
- **THEN** the Orchestrator adds that connection to its active worker pool and marks it as "Idle"

#### Scenario: Worker registers stable host identity
- **WHEN** a Worker process registers
- **THEN** the registration identifies the stable host whose local workspace and session stores the worker can access

#### Scenario: Worker registration omits host identity
- **WHEN** remote-worker placement is enabled
- **AND** a Worker process registers without a host ID
- **THEN** the Orchestrator rejects the registration

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

#### Scenario: Pinned workflow host is lost
- **WHEN** the Workflow Engine needs to execute a task for a pinned workflow instance
- **AND** the pinned host is lost or has no eligible registered worker
- **THEN** the Orchestrator marks the workflow instance as failed

### Requirement: Connection Failure Detection
The Orchestrator SHALL detect when a worker connection is closed or lost and update dispatch lease state for any task owned by that connection.

#### Scenario: Worker disconnects while idle
- **WHEN** an "Idle" worker closes its socket connection
- **THEN** the Orchestrator removes the worker from the pool

#### Scenario: Worker disconnects while busy
- **WHEN** a "Busy" worker closes its socket connection before sending a result
- **THEN** the Orchestrator removes the worker from the pool
- **THEN** the Orchestrator expires or releases the task dispatch lease according to recovery policy

### Requirement: Orchestrator Startup Recovery
The Orchestrator SHALL synchronize its internal task queue, workflow host pins, and in-flight dispatch leases with durable storage upon application startup.

#### Scenario: Orchestrator recovers tasks after crash
- **WHEN** the Orchestrator application starts up
- **THEN** it queries durable workflow and dispatch state for all non-terminal runnable or in-flight work
- **THEN** it restores pending work to its internal dispatch queue with persisted workflow pin constraints

#### Scenario: Orchestrator expires stale dispatches after crash
- **WHEN** the Orchestrator application starts up
- **AND** durable dispatch state contains an expired in-flight lease
- **THEN** it expires that lease and requeues or fails the task according to recovery policy
