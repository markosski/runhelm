## Context

The `WorkflowEngine` executes a DAG of `TaskInstance`s, resolving data-binding dependencies and validating outputs against JSON Schemas. Currently, the actual execution step is delegated to `simulate_task_execution`, a private method that returns a hardcoded `{"result": "success"}` regardless of the task's kind or schema. This is a placeholder that must be replaced before the engine can be used with real workloads.

The engine already depends on a `StoragePort` injected via `Arc<dyn StoragePort>`. The same pattern applies here: execution should be injected as a port, keeping the engine decoupled from any concrete runtime (Docker, process, HTTP, etc.).

Existing constraints:
- All orchestrator code is Rust.
- The engine is synchronous at the loop level but uses `async/await` for I/O (storage). Task execution will also be async.
- JSON Schema validation is already handled by the `jsonschema` crate inside the engine loop; the executor does **not** perform validation — it only produces a raw JSON value.
- The `TaskDef` carries all the information needed to dispatch a task: `kind` (discriminated union of `ApiCall` and `Agent`), `input_schemas`, `output_schema`, and `expected_side_effects`.

## Goals / Non-Goals

**Goals:**
- Define `ExecutorPort` — an async trait the engine calls to execute a single task.
- Implement `FakeExecutor` — produces schema-conformant JSON output using typed defaults; used in all unit tests and dry-run scenarios.
- Implement `DockerExecutor` — dispatches a task to a Docker container via the `bollard` crate; collects stdout as JSON.
- Refactor `WorkflowEngine` to accept `Arc<dyn ExecutorPort>` and remove `simulate_task_execution`.
- Keep the engine's validation and data-propagation logic entirely unchanged.

**Non-Goals:**
- Async/concurrent task execution within a single workflow run (tasks still run sequentially in the loop for now).
- Process-based or HTTP-based executors (future change).
- Retry logic, timeouts, or circuit-breaking within the executor (future change).
- Container image building or registry authentication.

## Decisions

### Decision 1: `ExecutorPort` signature — what does the engine pass in?

The engine knows the full `TaskDef` and the resolved `input_data` (a `serde_json::Value` array). Those two things are all an executor needs.

```
async fn execute(task: &TaskDef, inputs: &[serde_json::Value]) -> anyhow::Result<serde_json::Value>
```

**Why `&[serde_json::Value]` for inputs?** The engine already assembles inputs as an ordered array via `DataBinding.target_input_index`. Passing a slice makes the contract obvious and avoids re-keying by name.

**Why return `anyhow::Result<Value>` rather than a custom error enum?** Executor errors will surface as workflow failures with a diagnostic message. A typed error enum provides no actionable benefit at this stage and would add ceremony to every implementation. We can refine later.

**Alternative considered**: Passing the full `TaskInstance` — rejected because it exposes mutable state the executor should not touch.

---

### Decision 2: `FakeExecutor` — how to generate schema-conformant defaults

The fake executor needs to produce a `serde_json::Value` that passes `jsonschema::validator_for(&task.output_schema).is_valid(...)`. The simplest approach is a recursive schema walker:

| JSON Schema type | Default value |
|---|---|
| `"object"` | `{}` (populate required fields recursively) |
| `"string"` | `""` |
| `"number"` / `"integer"` | `0` |
| `"boolean"` | `false` |
| `"array"` | `[]` |
| `"null"` | `null` |
| no `type` / `{}` | `{}` |

For objects: iterate `required` fields and recursively generate defaults from `properties`. Fields not in `required` are omitted. `additionalProperties: false` is satisfied because we only emit declared required fields.

**Why not just return `{}`?** An empty object passes `{"type": "object"}` but fails strict schemas with `required` fields — which are exactly the schemas worth testing against.

**Alternative considered**: Use a JSON Schema faker library. None exists in the Rust ecosystem at sufficient maturity; a small recursive walker is 30–50 lines and fully under our control.

---

### Decision 3: `DockerExecutor` — input injection and output collection

**Input injection**: Construct a `TaskInvocationPayload` struct containing the full `TaskDef` and the resolved inputs array, serialize it as a single JSON object, and write it to the container's **stdin**. The container reads one JSON object from stdin and has everything it needs — task kind, kind-specific parameters (URL, agent ID, prompt, etc.), and the resolved inputs — without any out-of-band configuration.

```
struct TaskInvocationPayload {
    task: TaskDef,
    inputs: Vec<serde_json::Value>,
}
```

Wire format:
```json
{ "task": { "id": "...", "kind": { "ApiCall": { "url": "...", "method": "GET" } }, ... }, "inputs": [...] }
```

**Why include the full `TaskDef` rather than a trimmed envelope?** The `TaskDef` already derives `Serialize`/`Deserialize`, so it costs nothing extra. Trimming it would require a separate `TaskInvocationKind` type and mapping logic. We can slim the payload in a future change once the image contract stabilises. The container ignores fields it doesn't need.

**Output collection**: Container images write a single `TaskExecutionResult` JSON envelope to stdout. `DockerExecutor` collects stdout, parses it, and maps it to the `ExecutorPort` return type:

```
enum TaskExecutionResult {
    Ok  { output: serde_json::Value },
    Err { message: String, code: Option<String> },
}
```

Wire format:
```json
// Success
{ "status": "ok", "output": { ... } }

// Task-level error
{ "status": "error", "message": "Agent timed out", "code": "AGENT_TIMEOUT" }
```

**Why a structured envelope rather than raw JSON + exit code?** Exit codes alone cannot carry structured error context. Without an envelope, a container that fails must stuff its error message into stderr, making it impossible to distinguish "the task ran and produced a business error" from "the container itself crashed." The envelope makes both cases explicit and machine-readable. The `code` field in particular allows the engine (or a future retry policy) to react differently to different error classes.

**Two failure modes distinguished**:
- **Task-level failure**: stdout contains a valid `TaskExecutionResult` with `status: "error"` → `Err` with the envelope's message/code.
- **Infrastructure failure**: stdout is absent or not a valid envelope (container crash, OOM kill, etc.) → `Err` describing the parse failure plus raw stderr.

**Image resolution**: `DockerExecutor` will look up the container image name from `TaskDef.kind`. For the initial implementation only `ApiCall` and `Agent` variants exist; the executor will map these to a configurable image via a `HashMap<String, String>` passed at construction time (keyed by task kind name or a future `image` field on `TaskDef`). This is a temporary bridge until `TaskDef` grows an explicit `image` field.

**Why `bollard`?** It is the de-facto Rust Docker client, actively maintained, async-native, and wraps the Docker Engine API directly without shelling out. Shelling out to `docker run` was considered and rejected due to poor error handling and process lifecycle management.

**Container lifecycle**: `DockerExecutor` will:
1. `create_container` with stdin open, stdout/stderr attached.
2. `start_container`.
3. Write serialized inputs to stdin via the attach stream, then close it.
4. `wait_container` for exit code.
5. Collect stdout/stderr via log stream.
6. Return parsed stdout or an error wrapping stderr + exit code.

Containers are removed (`remove_container`) after collection regardless of outcome.

---

### Decision 4: Where does `ExecutorPort` live?

`src/ports/executor.rs` — consistent with `StoragePort` at `src/ports/storage.rs`. This is a primary port in Hexagonal Architecture terms: it defines what the engine needs, not how it is provided.

Implementations live in `src/adapters/`:
- `src/adapters/fake_executor.rs`
- `src/adapters/docker_executor.rs`

---

### Decision 5: `WorkflowEngine` constructor change

`WorkflowEngine::new` gains a second parameter:

```rust
pub fn new(
    storage: Arc<dyn StoragePort + Send + Sync>,
    executor: Arc<dyn ExecutorPort + Send + Sync>,
) -> Self
```

All existing tests will be updated to pass `Arc::new(FakeExecutor::new())`. No builder pattern is needed at this stage.

## Risks / Trade-offs

- **`FakeExecutor` correctness depends on schema structure** — highly dynamic schemas (e.g., `oneOf`, `anyOf`, `$ref`) are not handled in the initial recursive walker. Mitigation: limit recursive default generation to `type: object/string/number/integer/boolean/array/null`; for anything else, return `{}` and let the engine's validator surface the mismatch in tests, which makes the gap visible rather than silently wrong.

- **`DockerExecutor` requires a running Docker daemon** — integration tests for `DockerExecutor` cannot be unit-tested in the same way as `FakeExecutor`. Mitigation: mark Docker integration tests with `#[ignore]` by default; they run explicitly in CI with Docker-in-Docker. The `ExecutorPort` trait makes it trivial to swap in `FakeExecutor` for unit tests.

- **Input injection via stdin couples task images to a specific protocol** — images must read a JSON array from stdin. Mitigation: document this as the RunHelm task image contract; provide a minimal SDK/example. This is a deliberate protocol choice, not an accident.

- **Sequential execution loop** — the engine currently runs tasks one at a time even if multiple are `Ready`. `DockerExecutor` will add real latency per task. Mitigation: out of scope for this change; concurrency is a future engine concern. `FakeExecutor` is instant, so existing tests remain fast.

## Open Questions

- Should `TaskDef` grow an explicit `image: Option<String>` field now, or should `DockerExecutor` use a constructor-time image map? The image map is a workable bridge but the right long-term home is `TaskDef`. Recommend adding it in a follow-on change so this one stays focused.
- Do we want `DockerExecutor` to support pulling images automatically (`create_image`) if they are not present locally, or fail fast and require pre-pulled images? Fail-fast is safer for a first implementation.
