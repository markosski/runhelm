## 1. Workflow Model Updates

- [x] 1.1 Add `TaskControl` / `VerifierControlConfig` models under `TaskDef.control.verifier` with `max_iterations`, `on_exhausted_continue`, and optional `rerun_from_task_id`.
- [x] 1.2 Add materialized task attempt metadata so each `TaskInstance` retains stable attempt ID, original task definition ID, generation index, satisfaction state, and input mapping.
- [x] 1.3 Add persisted verifier generation state keyed by verifier task ID, including rerun start task ID, latest generation, selected generation, feedback history, verifier status, and exit reason.
- [x] 1.4 Add optional verifier metadata to verifier attempts while omitting verifier metadata for tasks without `control.verifier`.
- [x] 1.5 Add serde defaults so workflow definitions and workflow instances without verifier fields remain compatible.

## 2. Definition Validation

- [x] 2.1 Normalize workflow definition IDs and task definition IDs to lowercase when registering workflow definitions.
- [x] 2.2 Reject workflow definition IDs and task definition IDs that contain anything other than ASCII alphanumeric characters.
- [x] 2.3 Validate that verifier `max_iterations` is positive.
- [x] 2.4 Validate that optional `rerun_from_task_id` references an existing task.
- [x] 2.5 Validate that `rerun_from_task_id` is either omitted, equal to the verifier task ID, or an upstream ancestor of the verifier task.
- [x] 2.6 Reject custom `output_schema` on `control.verifier` tasks and inject the standard verifier decision schema during registration.
- [x] 2.7 Preserve ordinary data binding cycle validation while allowing only explicit verifier-controlled bounded backedges.
- [x] 2.8 Reject overlapping or nested verifier rerun slices.
- [x] 2.9 Keep `control.verifier` task-kind-neutral so Agent, Function, and API tasks can act as verifier control points.

## 3. Engine Scheduling

- [x] 3.1 Compute verifier-controlled rerun slices from optional `rerun_from_task_id` to the verifier task, defaulting to verifier self-rerun when omitted.
- [x] 3.2 Initialize generation-1 materialized attempts for the static workflow graph.
- [x] 3.3 Materialize later generation IDs only for rerun-slice tasks, such as `b[2]`, `c[2]`, and `d[2]`.
- [x] 3.4 Parse verifier task output decisions `continue` and `complete`, treating invalid verifier output as workflow failure.
- [x] 3.5 Require non-empty `feedback` for verifier decision `continue`.
- [x] 3.6 Store schema-valid rejected generations as lifecycle `Completed` task attempts with unsatisfied state and verifier metadata.
- [x] 3.7 Create the next rerun-slice generation when the verifier returns `continue` and has remaining iteration budget.
- [x] 3.8 Mark the workflow failed when an exhausted verifier uses `on_exhausted_continue: false`.
- [x] 3.9 Finalize the latest schema-valid generation when an exhausted verifier uses `on_exhausted_continue: true`.
- [x] 3.10 Mark the workflow failed when `on_exhausted_continue: true` is selected but no schema-valid latest generation output is available.

## 4. Loop Context and Binding Resolution

- [x] 4.1 Build orchestrator-owned loop context containing generation, max iterations, ordered feedback history, and previous same-task output.
- [x] 4.2 Inject loop context into rerun-slice task execution as dedicated execution metadata, not as user-declared input data.
- [x] 4.3 Persist verifier feedback and verifier output after each verifier run.
- [x] 4.4 Update data binding resolution so tasks inside a rerun slice consume same-generation upstream outputs.
- [x] 4.5 Update data binding resolution so tasks outside a rerun slice consume latest satisfied outputs.
- [x] 4.6 Ensure rejected generations remain observable but do not satisfy downstream readiness after the verifier.
- [x] 4.7 Record `input_mapping` on materialized attempts so read APIs expose the exact source generations consumed.

## 5. Status and Result APIs

- [x] 5.1 Update workflow status reports to include materialized generation attempt IDs.
- [x] 5.2 Include enough verifier summary state in persisted/read models to distinguish accepted, exhausted, failed, and running verifier flows.
- [x] 5.3 Include task satisfaction state, input mapping, generation index, and verifier metadata in task status/read metadata.
- [x] 5.4 Update task result lookup for a logical task ID to resolve a materialized attempt and include the resolved attempt ID.
- [x] 5.5 Update task result lookup for a materialized generation request to return that exact historical attempt and verifier metadata when present.
- [x] 5.6 Ensure non-verified task result lookup remains backward compatible.
- [x] 5.7 Ensure startup recovery resets materialized generation attempts left `Running` or verifying back to a resumable state.

## 6. Tests and Examples

- [x] 6.1 Add model serialization tests proving workflows without `control.verifier` still deserialize and execute unchanged.
- [x] 6.2 Add validation tests proving Function tasks can declare `control.verifier` and receive the injected decision schema.
- [x] 6.3 Add validation tests proving verifier decision schema injection and custom verifier `output_schema` rejection.
- [x] 6.4 Add validation tests for workflow definition/task definition ID lowercasing and alphanumeric-only rejection.
- [x] 6.5 Add validation tests rejecting missing, downstream, or unrelated `rerun_from_task_id` values.
- [x] 6.6 Add validation tests rejecting overlapping verifier rerun slices.
- [x] 6.7 Add engine tests for `A -> B -> C -> D(control.verifier)` accepted after the first generation.
- [x] 6.8 Add engine tests for verifier `continue` with feedback rerunning a slice as generation 2.
- [ ] 6.9 Add engine tests for exhaustion with `on_exhausted_continue: false` and `on_exhausted_continue: true`.
- [ ] 6.10 Add engine tests proving exhausted-continue fails when no schema-valid generation output exists.
- [x] 6.11 Add data binding tests proving same-generation propagation inside a rerun slice.
- [x] 6.12 Add data binding tests proving downstream tasks receive only accepted or finalized verifier generation outputs.
- [x] 6.13 Add result tests proving logical task ID lookup resolves to a materialized attempt and includes metadata.
- [x] 6.14 Add result tests proving materialized attempt listing returns exact historical attempts.
- [x] 6.15 Add status tests for materialized attempt IDs such as `task-a[1]` and `task-b[1]`.
- [x] 6.16 Add tests proving non-verified tasks have no verifier metadata.
- [x] 6.17 Add engine tests proving a Function verifier can return `continue` then `complete` and drive a bounded rerun.

## 7. Verification

- [x] 7.1 Run orchestrator unit tests with `cargo test`.
- [x] 7.2 Worker tests are not required for this artifact alignment because no worker payload or TypeScript model types changed.
- [x] 7.3 Run `openspec status --change add-bounded-workflow-loops` and confirm all artifacts are complete.
