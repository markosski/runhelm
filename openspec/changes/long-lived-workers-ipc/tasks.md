## 1. Orchestrator IPC Server

- [ ] 1.1 Implement `tokio::net::UnixListener` in the Orchestrator to bind to `/tmp/runhelm.sock`.
- [ ] 1.2 Define the NDJSON message types for registration and task dispatch.
- [ ] 1.3 Create a thread-safe `WorkerPool` registry to manage active connections.
- [ ] 1.4 Implement a connection handler that processes worker registration and adds them to the pool.

## 2. Worker IPC Client

- [ ] 2.1 Update `worker/src/index.ts` to connect to the Orchestrator socket upon startup.
- [ ] 2.2 Implement the worker registration handshake (identifying capabilities).
- [ ] 2.3 Refactor the worker's main loop to listen for task payloads from the socket instead of CLI arguments.
- [ ] 2.4 Ensure task results are serialized back to the socket as NDJSON.

## 3. Core Logic & Orchestration

- [ ] 3.1 Refactor the `orchestrator` crate to use the `WorkerPool` instead of spinning up new containers.
- [ ] 3.2 Implement a task timeout watchdog that monitors "Busy" workers and marks tasks as failed if no response arrives.
- [ ] 3.3 Add robust error handling for dropped socket connections during task execution.
- [ ] 3.4 Cleanup: Remove unused Docker SDK dependencies and legacy container management code from the Orchestrator.
- [ ] 3.5 Implement startup synchronization: Query Postgres for PENDING/EXECUTING tasks and re-populate the internal dispatch queue.

## 4. Infrastructure & Validation

- [ ] 4.1 Update the project's `docker-compose.yml` to share a common `/tmp` mount between Orchestrator and Workers.
- [ ] 4.2 Set up environment variables for the socket path across all components.
- [ ] 4.3 Create a manual test script to verify that multiple workers can join the pool and execute tasks concurrently.
