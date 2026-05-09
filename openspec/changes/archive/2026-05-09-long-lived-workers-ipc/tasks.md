## 1. Orchestrator IPC Server

- [x] 1.1 Implement `tokio::net::UnixListener` in the Orchestrator to bind to `/tmp/runhelm.sock`.
- [x] 1.2 Define the NDJSON message types for registration and task dispatch.
- [x] 1.3 Create a thread-safe `WorkerPool` registry to manage active connections.
- [x] 1.4 Implement a connection handler that processes worker registration and adds them to the pool.

## 2. Worker IPC Client

- [x] 2.1 Update `worker/src/index.ts` to connect to the Orchestrator socket upon startup.
- [x] 2.2 Implement the worker registration handshake (identifying capabilities).
- [x] 2.3 Refactor the worker's main loop to listen for task payloads from the socket instead of CLI arguments.
- [x] 2.4 Ensure task results are serialized back to the socket as NDJSON.

## 3. Core Logic & Orchestration

- [x] 3.1 Refactor the `orchestrator` crate to use the `WorkerPool` instead of spinning up new containers.
- [x] 3.2 Implement a task timeout watchdog that monitors "Busy" workers and marks tasks as failed if no response arrives.
- [x] 3.3 Add robust error handling for dropped socket connections during task execution.
- [x] 3.4 Cleanup: Remove unused Docker SDK dependencies and legacy container management code from the Orchestrator.
- [x] 3.5 Implement startup synchronization: Query Postgres for PENDING/EXECUTING tasks and re-populate the internal dispatch queue.

## 4. Infrastructure & Validation

- [x] 4.1 Update the project's `docker-compose.yml` to share only the IPC socket location between Orchestrator and Workers, without bind-mounting the host `/tmp`.
- [x] 4.2 Set up environment variables for the socket path across all components.
- [x] 4.3 Create a manual test script to verify that multiple workers can join the pool and execute tasks concurrently.
