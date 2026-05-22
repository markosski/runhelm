## 1. Workflow Model Updates

- [ ] 1.1 Add Agent verifier configuration models to `orchestrator/src/core/models.rs`, including `max_iterations`, `on_exhausted_continue`, `on_failure_rerun_task`, and dependency-free verifier code.
- [ ] 1.2 Reject verifier configuration on non-Agent task definitions.
- [ ] 1.3 Add materialized generation metadata to `TaskInstance` or an adjacent model so rerun attempts retain stable attempt ID, original task definition ID, and generation index.
- [ ] 1.4 Add persisted verifier-controlled generation state keyed by verifier task ID, including rerun start task ID, latest generation, selected generation, feedback history, verifier status, and exit reason.
- [ ] 1.5 Add optional verifier metadata to verifier Agent generations while omitting verifier metadata for tasks without verifier blocks.
- [ ] 1.6 Add serde defaults so workflow definitions and workflow instances without verifier fields remain compatible.

## 2. Definition Validation

- [ ] 2.1 Normalize workflow IDs and task IDs to lowercase when registering workflow definitions.
- [ ] 2.2 Reject workflow IDs and task IDs that contain anything other than ASCII alphanumeric characters.
- [ ] 2.3 Validate that verifier `max_iterations` is positive.
- [ ] 2.4 Validate that `on_exhausted_continue` is boolean and `on_failure_rerun_task` references an existing task.
- [ ] 2.5 Validate that `on_failure_rerun_task` is an upstream ancestor of the verifier Agent task.
- [ ] 2.6 Validate that verifier code is inline and does not declare external dependencies.
- [ ] 2.7 Preserve ordinary data binding cycle validation while allowing only explicit verifier-controlled bounded backedges.

## 3. Engine Scheduling

- [ ] 3.1 Compute verifier-controlled rerun slices from `on_failure_rerun_task` to the verifier task.
- [ ] 3.2 Update workflow initialization so first-generation rerun-slice task instances are materialized when the slice becomes eligible to run.
- [ ] 3.3 Implement materialized generation ID generation for rerun-slice tasks, such as `b[1]`, `c[1]`, `d[1]`, then `b[2]`, `c[2]`, `d[2]`.
- [ ] 3.4 Execute verifier code in the worker after the verifier Agent generation produces schema-valid output.
- [ ] 3.5 Pass verifier context containing output, generation, max iterations, feedback history, and selected upstream context.
- [ ] 3.6 Parse verifier decisions `continue` and `complete`, treating invalid verifier output as workflow failure.
- [ ] 3.7 Require non-empty `feedback` for verifier decision `continue`.
- [ ] 3.8 Store schema-valid rejected generations as lifecycle `Completed` task attempts with verifier status metadata.
- [ ] 3.9 Create the next rerun-slice generation when the verifier returns `continue` and the verifier has remaining iteration budget.
- [ ] 3.10 Mark the workflow failed when an exhausted verifier uses `on_exhausted_continue: false`.
- [ ] 3.11 Finalize the latest schema-valid generation when an exhausted verifier uses `on_exhausted_continue: true`.
- [ ] 3.12 Mark the workflow failed when `on_exhausted_continue: true` is selected but no schema-valid latest generation output is available.

## 4. Loop Context and Binding Resolution

- [ ] 4.1 Build orchestrator-owned loop context containing generation, max iterations, latest feedback, and feedback history.
- [ ] 4.2 Inject loop context into rerun-slice task execution as dedicated execution metadata, not as user-declared input data.
- [ ] 4.3 Persist verifier feedback and verifier output after each verifier run.
- [ ] 4.4 Update data binding resolution so tasks inside a rerun slice consume same-generation upstream outputs.
- [ ] 4.5 Update data binding resolution so tasks outside a rerun slice consume selected outputs from accepted or exhaustion-finalized generations.
- [ ] 4.6 Ensure rejected generations remain observable but do not satisfy downstream readiness after the verifier.

## 5. Status and Result APIs

- [ ] 5.1 Update workflow status reports to include materialized generation attempt IDs.
- [ ] 5.2 Include enough verifier summary state in persisted/read models to distinguish accepted, exhausted, failed, and running verifier flows.
- [ ] 5.3 Update task result lookup for a logical task ID in a rerun slice to resolve the selected generation and include the resolved attempt ID.
- [ ] 5.4 Update task result lookup for a materialized attempt ID to return that exact historical attempt and verifier metadata when present.
- [ ] 5.5 Ensure non-verified task result lookup remains backward compatible.
- [ ] 5.6 Ensure startup recovery resets materialized generation attempts left `Running` or verifying back to a resumable state.

## 6. Tests and Examples

- [ ] 6.1 Add model serialization tests proving workflows without verifier blocks still deserialize and execute unchanged.
- [ ] 6.2 Add validation tests rejecting verifier blocks on non-Agent tasks.
- [ ] 6.3 Add validation tests rejecting verifier dependency declarations.
- [ ] 6.4 Add validation tests for workflow/task ID lowercasing and alphanumeric-only rejection.
- [ ] 6.5 Add validation tests rejecting missing, downstream, or unrelated `on_failure_rerun_task` values.
- [ ] 6.6 Add worker verifier tests for context shape and result validation, including required feedback on `continue`.
- [ ] 6.7 Add engine tests for `A -> B -> C -> D(verifier)` accepted after the first generation.
- [ ] 6.8 Add engine tests for `D` continuing with feedback and rerunning `B -> C -> D` as generation 2.
- [ ] 6.9 Add engine tests for exhaustion with `on_exhausted_continue: false` and `on_exhausted_continue: true`.
- [ ] 6.10 Add engine tests proving exhausted-continue fails when no schema-valid generation output exists.
- [ ] 6.11 Add data binding tests proving same-generation propagation inside a rerun slice.
- [ ] 6.12 Add data binding tests proving downstream tasks receive only the accepted or finalized verifier generation output.
- [ ] 6.13 Add result tests proving logical task ID lookup resolves to the selected generation.
- [ ] 6.14 Add result tests proving materialized attempt ID lookup returns the exact historical attempt.
- [ ] 6.15 Add status tests for materialized attempt IDs such as `b[1]`, `c[1]`, `d[1]`, and `b[2]`.
- [ ] 6.16 Add tests proving non-verified tasks have no verifier metadata.
- [ ] 6.17 Add an example workflow YAML demonstrating `A -> B -> C -> D(verifier)` rerunning from `B`.

## 7. Verification

- [ ] 7.1 Run orchestrator unit tests with `cargo test`.
- [ ] 7.2 Run worker tests if task payload or TypeScript model types are updated.
- [ ] 7.3 Run `openspec status --change add-bounded-workflow-loops` and confirm all artifacts are complete.
