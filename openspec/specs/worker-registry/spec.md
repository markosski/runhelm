# Capability: worker-registry

## Purpose
Defines the behavior of `WorkerRegistry`, the in-memory component responsible for worker registration, heartbeat state, missed heartbeat detection, deregistration, and eligible host selection.

## Requirements

### Requirement: Worker Registration
`WorkerRegistry` SHALL track active worker identities separately from durable worker host identities.

#### Scenario: Worker registers
- **WHEN** a worker registers with a worker ID and host ID
- **THEN** the registry SHALL store that worker identity
- **AND** the registry SHALL mark the worker heartbeat state healthy

#### Scenario: Multiple workers on one host
- **WHEN** two worker IDs register with the same host ID
- **THEN** the registry SHALL track both workers independently
- **AND** eligible host selection SHALL return the host once

### Requirement: Heartbeat Renewal
Worker heartbeats SHALL renew the worker's liveness registration.

#### Scenario: Healthy heartbeat
- **WHEN** a registered worker sends a heartbeat before deregistration
- **THEN** the registry SHALL refresh the worker heartbeat deadlines
- **AND** the worker SHALL be eligible to claim tasks

#### Scenario: Rejoin after deregistration
- **WHEN** a previously deregistered worker sends a heartbeat
- **THEN** the registry SHALL create a fresh registration for that worker identity

### Requirement: Missed Heartbeat Detection
`WorkerRegistry` SHALL mark workers unavailable for task claims after their next heartbeat deadline passes.

#### Scenario: Worker misses heartbeat
- **WHEN** a worker's next heartbeat deadline has passed
- **THEN** the registry SHALL mark the worker as having missed heartbeat
- **AND** claim validation for that worker SHALL fail

#### Scenario: Heartbeat clears missed state
- **WHEN** a worker marked as missed sends a new heartbeat before deregistration
- **THEN** the registry SHALL clear the missed heartbeat state
- **AND** claim validation for that worker SHALL succeed

### Requirement: Lost Host Detection
`WorkerRegistry` SHALL deregister workers after the missed heartbeat threshold and report durable hosts with no remaining registered workers.

#### Scenario: Last worker on host deregisters
- **WHEN** the final worker registration for a host exceeds the deregistration deadline
- **THEN** the registry SHALL return that host ID as lost

#### Scenario: Host still has another worker
- **WHEN** one worker registration on a host deregisters but another worker on the same host remains registered
- **THEN** the registry SHALL NOT return that host ID as lost

### Requirement: Eligible Host Selection
`WorkerRegistry` SHALL expose eligible host selection for workflow pinning and force retry.

#### Scenario: Trigger selects eligible host
- **WHEN** at least one registered worker host has no missed heartbeat
- **THEN** the registry SHALL return a deterministic eligible host ID

#### Scenario: Force retry preserves eligible current host
- **WHEN** force retry is requested and the current pinned host is still eligible
- **THEN** the registry SHALL return the current pinned host

#### Scenario: Force retry reassigns unavailable current host
- **WHEN** force retry is requested and the current pinned host is not eligible
- **THEN** the registry SHALL return another eligible host if one exists

#### Scenario: No eligible host
- **WHEN** no registered worker host is eligible
- **THEN** eligible host selection SHALL return no host
