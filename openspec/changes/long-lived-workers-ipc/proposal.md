## Why

The current `DockerExecutor` implementation spins up a new container and Node.js environment for every single task. This causes severe cold-start latency (seconds per task), making agentic workflows with many small steps unacceptably slow and inefficient.

## What Changes

- **BREAKING**: Replace the "ephemeral container per task" execution model with a "long-lived worker pool" model.
- **Orchestrator IPC Server**: Implement a Unix Domain Socket server in the Orchestrator to handle worker connections.
- **Worker IPC Client**: Modify the Worker to connect as a client to the Orchestrator's IPC socket upon startup and wait for tasks.
- **Connection Management**: Implement logic in the Orchestrator to manage a pool of active worker connections and dispatch tasks to idle workers.
- **Manual Task Acknowledgement**: Move to a model where workers immediately acknowledge task receipt, and the Orchestrator manages task timeouts and retries via a watchdog mechanism.

## Capabilities

### New Capabilities
- `worker-pool-ipc`: Management of the persistent worker pool, connection lifecycle, and task dispatching via Unix Domain Sockets.

### Modified Capabilities
- `executor-port`: Update the executor interface to support persistent connections and asynchronous task status tracking.

## Impact

- **Orchestrator (Rust)**: Significant changes to the `orchestrator` crate to include an IPC server and a connection-aware worker pool.
- **Worker (TypeScript)**: Modification of the `worker` entry point to support persistent socket connections instead of one-off CLI execution.
- **Infrastructure**: Updates to `docker-compose` and deployment scripts to maintain a fixed number of warm worker containers.
