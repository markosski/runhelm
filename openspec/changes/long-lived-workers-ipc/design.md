## Context

RunHelm currently uses an ephemeral container model where each task execution spins up a fresh Docker container and Node.js environment. This results in several seconds of latency per task. This design introduces a persistent IPC-based worker pool to eliminate this overhead.

## Goals / Non-Goals

**Goals:**
- Eliminate container startup latency (aiming for <10ms task dispatch overhead).
- Implement a robust "Reverse Connection" model where workers connect to a central Orchestrator socket.
- Support multiple concurrent workers on a single host.
- Centralize task timeout and lifecycle management in the Orchestrator.

**Non-Goals:**
- Implementation of a distributed message queue (SQS/Redis) is deferred in favor of IPC for simplicity and speed.
- Dynamic worker auto-scaling (scaling is handled at the infrastructure/EC2 level).

## Decisions

### 1. Reverse Connection Model (Workers as Clients)
**Decision**: The Orchestrator will act as a Unix Domain Socket server, and workers will act as clients that connect upon startup.
- **Rationale**: In a Docker environment, it is easier for workers to connect to a shared socket mount point than for the Orchestrator to know the identity/address of $N$ dynamically started worker containers.
- **Supervisor Role**: The Orchestrator will no longer manage the Docker lifecycle (spawning/killing containers). Instead, it will be a "Passive Server."
- **Infrastructure**: Docker Compose (or K8s) is responsible for spawning workers and ensuring they stay alive (e.g., via `restart: always`).
- **Alternatives Considered**: 
    - *Workers as Servers*: Orchestrator connects to each worker. Rejected due to the complexity of discovering worker identities and socket paths.
    - *Shared Request Queue (Redis)*: Rejected to minimize infrastructure dependencies for the initial version.
    - *Orchestrator as Supervisor*: Orchestrator using Docker SDK to manage the fleet. Rejected to keep the Orchestrator codebase clean and cloud-agnostic.

### 2. Protocol: Newline-Delimited JSON (NDJSON)
**Decision**: Communication over the socket will use NDJSON framing.
- **Rationale**: Simple to parse in both Rust (Tokio/Serde) and TypeScript (Node.js Streams). It provides a natural way to frame messages in a continuous stream.
- **Alternatives Considered**: 
    - *gRPC over UDS*: Rejected as overkill for simple JSON dispatching.
    - *Binary Protobuf*: Rejected to maintain human-readability for debugging.

### 3. Orchestrator-Managed Worker Pool
**Decision**: The Orchestrator will maintain a thread-safe registry of active connections, tracking which workers are "Idle" or "Busy."
- **Rationale**: Centralizing the pool logic in the Orchestrator allows it to implement sophisticated load balancing, task-stealing (if needed later), and timeout management.

## Risks / Trade-offs

- **[Risk] Orchestrator Restart Persistence** → **[Mitigation]**: The Orchestrator treats the database as the **Source of Truth.** Upon startup, it scans for `PENDING` or `EXECUTING` tasks and re-populates its internal async queue.
- **[Risk] Worker Crash during Execution** → **[Mitigation]**: If the IPC connection is severed while a task is in progress, the Orchestrator catches the `BrokenPipe` error, removes the worker from the pool, and updates the task status in the database to allow for retry.
- **[Risk] Socket Permissions** → **[Mitigation]**: Ensure the `docker-compose` setup correctly sets ownership of the `/tmp/runhelm.sock` so both the Orchestrator container and Worker containers have read/write access.
- **[Risk] Zombie Processes** → **[Mitigation]**: If a worker hangs, the Orchestrator will eventually timeout the task. Since we are reusing workers, the Orchestrator will "evict" the connection and ignore any late results, while the infrastructure (Docker) will eventually recycle the container if health checks fail.

## Reliability Architecture: The "Truth" vs. "Performance"
To achieve both high performance and high reliability, the system follows these rules:
1. **The Database is the Truth**: Every state change (`PENDING` -> `EXECUTING` -> `COMPLETED`) is committed to Postgres first.
2. **The Memory is for Performance**: The Orchestrator's internal async queue is just a high-speed "cache" of the work that needs to be done.
3. **Event-Driven Recovery**: The Orchestrator doesn't need to poll the DB for new work. It only reads the DB at **Startup**. After that, it relies on API events and IPC connection events.
