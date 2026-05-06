## ADDED Requirements

### Requirement: Orchestrator IPC Server
The Orchestrator SHALL host a Unix Domain Socket server to receive connections from long-lived worker processes.

#### Scenario: Orchestrator starts IPC server
- **WHEN** the Orchestrator application starts up
- **THEN** it binds to a Unix Domain Socket at the path specified in the configuration (defaulting to `/tmp/runhelm.sock`)

### Requirement: Worker Connection and Registration
Workers SHALL connect to the Orchestrator's socket and provide a "registration" message identifying their capabilities and identity.

#### Scenario: Successful worker registration
- **WHEN** a Worker process connects to the socket and sends a valid registration JSON
- **THEN** the Orchestrator adds that connection to its active worker pool and marks it as "Idle"

### Requirement: Exclusive Task Dispatching
The Orchestrator SHALL dispatch a task to exactly one idle worker from the pool.

#### Scenario: Dispatching a task to an idle worker
- **WHEN** the Workflow Engine needs to execute a task and there is at least one "Idle" worker in the pool
- **THEN** the Orchestrator sends the task JSON to the worker's socket and marks that worker as "Busy"

### Requirement: Task Completion via Response Queue
Workers SHALL send the task result back to the Orchestrator via the same socket connection.

#### Scenario: Worker returns task result
- **WHEN** a worker finishes a task and writes the result JSON back to the socket
- **THEN** the Orchestrator receives the result, routes it to the correct workflow task, and marks the worker as "Idle" again

### Requirement: Connection Failure Detection
The Orchestrator SHALL detect when a worker connection is closed or lost.

#### Scenario: Worker disconnects while idle
- **WHEN** an "Idle" worker closes its socket connection
- **THEN** the Orchestrator removes the worker from the pool

#### Scenario: Worker disconnects while busy
- **WHEN** a "Busy" worker closes its socket connection before sending a result
- **THEN** the Orchestrator removes the worker from the pool and marks the task as "RETRY_PENDING" (or similar) in the database

### Requirement: Orchestrator Startup Recovery
The Orchestrator SHALL synchronize its internal task queue with the database upon application startup.

#### Scenario: Orchestrator recovers tasks after crash
- **WHEN** the Orchestrator application starts up
- **THEN** it queries the database for all tasks with "PENDING" or "EXECUTING" status and adds them to its internal dispatch queue

### Requirement: Task Timeout Management
The Orchestrator SHALL monitor the execution time of tasks assigned to workers and handle timeouts.

#### Scenario: Task exceeds maximum execution time
- **WHEN** a worker does not return a result within the configured task timeout
- **THEN** the Orchestrator marks the task as "FAILED" in the database and ignores any subsequent results from that specific connection
