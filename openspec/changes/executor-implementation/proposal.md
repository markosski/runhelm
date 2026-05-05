## Why

The `WorkflowEngine` currently calls `simulate_task_execution`, a hardcoded stub that returns a fixed JSON value regardless of task kind, input, or output schema. This makes the engine untestable with real workloads and blocks any meaningful integration with actual compute targets. Introducing a proper `Executor` port with concrete implementations is the next required step toward a production-capable orchestrator.

## What Changes

- Introduce an `ExecutorPort` trait (`src/ports/executor.rs`) that the engine calls to execute a single task, replacing `simulate_task_execution`.
- Refactor `WorkflowEngine` to accept an `Arc<dyn ExecutorPort>` dependency and call it during the execution loop.
- Implement `FakeExecutor` (`src/adapters/fake_executor.rs`) — always returns a valid JSON object that satisfies the task's `output_schema` by filling required fields with typed default values. Intended for testing and dry-run scenarios.
- Implement `DockerExecutor` (`src/adapters/docker_executor.rs`) — runs tasks inside a Docker container, passing input data via environment variables or stdin, and collecting JSON output from stdout.
- Update `WorkflowEngine::new` signature (or provide a builder) to accept an executor.
- Update existing engine tests to use `FakeExecutor` instead of the internal stub.

## Capabilities

### New Capabilities

- `task-executor`: Defines the `ExecutorPort` interface contract — how the engine requests task execution, what inputs it provides, and what outputs it expects in return.
- `fake-executor`: Specifies the behavior of the fake (default-value) executor, including how it generates schema-conformant default outputs for any well-formed JSON Schema.
- `docker-executor`: Specifies how tasks are dispatched to Docker containers, covering image resolution, input injection, output collection, timeout handling, and error mapping.

### Modified Capabilities

- `workflow-dataflow-engine`: The engine's execution loop requirement changes — rather than invoking an internal simulation, it SHALL delegate task execution to a pluggable `ExecutorPort`. The existing data-binding and validation requirements remain unchanged.

## Impact

- **`src/ports/executor.rs`** — new file, defines the `ExecutorPort` async trait.
- **`src/ports/mod.rs`** — expose the new `executor` module.
- **`src/adapters/fake_executor.rs`** — new file, `FakeExecutor` implementation.
- **`src/adapters/docker_executor.rs`** — new file, `DockerExecutor` implementation.
- **`src/adapters/mod.rs`** — expose the new adapter modules.
- **`src/core/engine.rs`** — `WorkflowEngine` gains an `executor: Arc<dyn ExecutorPort>` field; `simulate_task_execution` is removed.
- **`Cargo.toml`** — Docker executor will likely require an HTTP client (`reqwest`) or a Docker SDK crate (e.g., `bollard`) as an optional dependency.
- Existing engine unit tests must pass the `FakeExecutor` to `WorkflowEngine::new`.
