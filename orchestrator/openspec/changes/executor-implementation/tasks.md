## 1. ExecutorPort Trait

- [ ] 1.1 Create `src/ports/executor.rs` — define `ExecutorPort` as an async trait with `execute(&self, task: &TaskDef, inputs: &[serde_json::Value]) -> anyhow::Result<serde_json::Value>`
- [ ] 1.2 Expose `executor` module in `src/ports/mod.rs`

## 2. FakeExecutor Implementation

- [ ] 2.1 Create `src/adapters/fake_executor.rs` — implement `FakeExecutor` struct with `ExecutorPort`
- [ ] 2.2 Implement the recursive schema default walker: handle `object`, `string`, `number`, `integer`, `boolean`, `array`, `null`, and no-type fallback
- [ ] 2.3 For `object` schemas: iterate `required` field names and recursively generate defaults from `properties`; omit non-required fields
- [ ] 2.4 For unsupported constructs (`oneOf`, `anyOf`, `$ref`): return `{}` without error
- [ ] 2.5 Expose `fake_executor` module in `src/adapters/mod.rs`

## 3. WorkflowEngine Refactor

- [ ] 3.1 Add `executor: Arc<dyn ExecutorPort + Send + Sync>` field to `WorkflowEngine`
- [ ] 3.2 Update `WorkflowEngine::new` to accept `executor` as a second parameter
- [ ] 3.3 Replace `simulate_task_execution` call in the execution loop with `self.executor.execute(task_def, &inputs).await`
- [ ] 3.4 Resolve the task's current `input_data` into a `&[serde_json::Value]` slice before calling the executor (extract from the `Array` variant, or pass `&[]` if `None`)
- [ ] 3.5 Remove the `simulate_task_execution` method entirely
- [ ] 3.6 Update all existing engine unit tests to construct `WorkflowEngine::new(storage, Arc::new(FakeExecutor::new()))`
- [ ] 3.7 Verify all existing tests still pass: `cargo test`

## 4. DockerExecutor Implementation

- [ ] 4.1 Add `bollard` to `Cargo.toml` as a dependency (no extra features needed; connecting via the local Unix socket)
- [ ] 4.2 Create `src/adapters/docker_executor.rs` — define `DockerExecutor` struct holding a `bollard::Docker` client and an image map (`HashMap<String, String>`)
- [ ] 4.3 Implement `DockerExecutor::new(image_map: HashMap<String, String>) -> anyhow::Result<Self>` — connect to the local Docker daemon via `Docker::connect_with_local_defaults()`
- [ ] 4.4 Implement image resolution: match on `TaskDef.kind` to produce a string key (`"ApiCall"` / `"Agent"`), look up in image map, return `Err` if not found
- [ ] 4.5 Define `TaskInvocationPayload` struct (`task: TaskDef, inputs: Vec<serde_json::Value>`) in `src/adapters/docker_executor.rs` with `Serialize`/`Deserialize` derived
- [ ] 4.6 Define `TaskExecutionResult` enum in `src/adapters/docker_executor.rs` with variants `Ok { output: serde_json::Value }` and `Err { message: String, code: Option<String> }`; use `#[serde(tag = "status")]` with `#[serde(rename = "ok"/"error")]` on each variant — no custom deserializer needed
- [ ] 4.7 Implement container creation: call `create_container` with stdin open, stdout/stderr attached, using the resolved image name
- [ ] 4.8 Implement stdin injection: construct `TaskInvocationPayload { task, inputs }`, serialize to JSON, write to the container's attach stream, then close the write half so the container can detect EOF
- [ ] 4.9 Implement container start and wait: call `start_container`, then `wait_container` and collect the exit code
- [ ] 4.10 Implement stdout/stderr collection: collect log stream bytes into two separate `String` buffers (stdout, stderr)
- [ ] 4.11 Implement result resolution: parse stdout as `TaskExecutionResult`; on `status: "ok"` return `Ok(output)`; on `status: "error"` return `Err` with message and code; on parse failure return `Err` with raw stdout + stderr
- [ ] 4.12 Implement cleanup: call `remove_container` (with `force: true`) after result resolution, regardless of outcome; log but do not propagate cleanup errors
- [ ] 4.13 Expose `docker_executor` module in `src/adapters/mod.rs`

## 5. DockerExecutor Integration Tests

- [ ] 5.1 Create `tests/docker_executor_integration.rs` (or a `#[cfg(test)]` module in the adapter) with a test that uses a known minimal image (e.g., `alpine`) to verify the stdin→stdout JSON round-trip
- [ ] 5.2 Annotate all Docker integration tests with `#[ignore]` so they are skipped in standard `cargo test` runs
- [ ] 5.3 Verify integration tests pass when run explicitly: `cargo test -- --ignored`

## 6. FakeExecutor Unit Tests

- [ ] 6.1 Add unit tests for the schema walker covering: empty object, object with required fields, string, integer, number, boolean, array, null, and no-type schemas
- [ ] 6.2 Add a test verifying that `FakeExecutor` output passes `jsonschema::validator_for(&schema).is_valid(&output)` for a strict schema with `required` fields and `additionalProperties: false`
- [ ] 6.3 Add a test verifying the graceful fallback (`{}`) for a schema containing `oneOf`
