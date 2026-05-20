## 1. Workflow Model Updates

- [ ] 1.1 Add Agent verifier configuration models to `orchestrator/src/core/models.rs`, including max iterations, exhaustion policy, loop context input index, and dependency-free verifier code.
- [ ] 1.2 Reject verifier configuration on non-Agent task definitions.
- [ ] 1.3 Add persisted verified Agent attempt state, including attempt index, verifier decision, feedback history, exit reason, and accepted/finalized attempt tracking.
- [ ] 1.4 Add materialized attempt metadata to `TaskInstance` or an adjacent model so Agent attempts retain both stable attempt ID and original task definition ID.
- [ ] 1.5 Add a logical verified Agent state map keyed by original task ID to track attempt IDs, latest attempt, accepted/finalized attempt, feedback history, status, and exit reason.
- [ ] 1.6 Add optional verifier metadata to verified Agent attempts while omitting verifier metadata for tasks without verifier blocks.
- [ ] 1.7 Add serde defaults so workflow definitions and workflow instances without verifier fields remain compatible.

## 2. Definition Validation

- [ ] 2.1 Normalize workflow IDs and task IDs to lowercase when registering workflow definitions.
- [ ] 2.2 Reject workflow IDs and task IDs that contain anything other than ASCII alphanumeric characters.
- [ ] 2.3 Validate that verified Agent `max_iterations` is positive and `on_exhausted` is one of the supported policies.
- [ ] 2.4 Validate that verifier code is inline and does not declare external dependencies.
- [ ] 2.5 Validate that `loop_context_input_index` is compatible with the Agent task input schema when present.
- [ ] 2.6 Preserve ordinary data binding cycle validation; verified Agent attempts must not require cyclic data bindings.

## 3. Engine Scheduling

- [ ] 3.1 Update workflow initialization so verified Agent attempts are materialized when the Agent task becomes eligible to run.
- [ ] 3.2 Implement materialized attempt ID generation for verified Agent tasks, such as `implementchange[1]` and `implementchange[2]`.
- [ ] 3.3 Execute verifier code in the worker after a verified Agent attempt produces schema-valid output.
- [ ] 3.4 Pass verifier context containing only `output`, `attempt`, `max_iterations`, and `feedback_history`.
- [ ] 3.5 Parse verifier decisions `continue` and `complete`, treating invalid verifier output as workflow failure.
- [ ] 3.6 Require non-empty `feedback` for verifier decision `continue`.
- [ ] 3.7 Store schema-valid rejected attempts as lifecycle `Completed` with verifier status metadata.
- [ ] 3.8 Create the next Agent attempt when the verifier returns `continue` and the task has remaining iteration budget.
- [ ] 3.9 Mark the workflow failed when an exhausted verifier uses `on_exhausted: "fail"`.
- [ ] 3.10 Finalize the highest available schema-valid attempt when an exhausted verifier uses `on_exhausted: "continue"`.
- [ ] 3.11 Mark the workflow failed when `on_exhausted: "continue"` is selected but no schema-valid attempt output is available.

## 4. Loop Context and Binding Resolution

- [ ] 4.1 Build orchestrator-owned loop context containing iteration, max iterations, latest feedback, and feedback history.
- [ ] 4.2 Inject loop context into the verified Agent attempt at the configured input index.
- [ ] 4.3 Persist verifier feedback and verifier output after each verifier run.
- [ ] 4.4 Update data binding resolution so downstream bindings from a verified Agent use only the highest accepted or exhaustion-finalized attempt.
- [ ] 4.5 Ensure rejected Agent attempts remain observable but do not satisfy downstream readiness or workflow completion.

## 5. Status and Result APIs

- [ ] 5.1 Update workflow status reports to include materialized verified Agent attempt IDs.
- [ ] 5.2 Include enough verifier summary state in persisted/read models to distinguish accepted, exhausted, failed, and running verifier flows.
- [ ] 5.3 Update task result lookup for a logical verified Agent task ID to resolve the accepted or exhaustion-finalized attempt and include the resolved attempt ID.
- [ ] 5.4 Update task result lookup for a materialized attempt ID to return that exact historical attempt and verifier metadata.
- [ ] 5.5 Ensure non-verified task result lookup remains backward compatible.
- [ ] 5.6 Ensure startup recovery resets materialized Agent attempts left `Running` or verifying back to a resumable state.

## 6. Tests and Examples

- [ ] 6.1 Add model serialization tests proving workflows without verifier blocks still deserialize and execute unchanged.
- [ ] 6.2 Add validation tests rejecting verifier blocks on non-Agent tasks.
- [ ] 6.3 Add validation tests rejecting verifier dependency declarations.
- [ ] 6.4 Add validation tests for workflow/task ID lowercasing and alphanumeric-only rejection.
- [ ] 6.5 Add worker verifier tests for context shape and result validation, including required feedback on `continue`.
- [ ] 6.6 Add engine tests for a verified Agent accepted after the first attempt.
- [ ] 6.7 Add engine tests for a verified Agent that continues with feedback and accepts a second attempt.
- [ ] 6.8 Add engine tests for exhaustion with `on_exhausted: "fail"` and `on_exhausted: "continue"`.
- [ ] 6.9 Add engine tests proving exhausted-continue fails when no schema-valid attempt output exists.
- [ ] 6.10 Add data binding tests proving downstream tasks receive only the highest accepted or finalized Agent attempt output.
- [ ] 6.11 Add result tests proving logical task ID lookup resolves to the accepted/finalized attempt.
- [ ] 6.12 Add result tests proving materialized attempt ID lookup returns the exact historical attempt.
- [ ] 6.13 Add status tests for materialized attempt IDs such as `implementchange[1]` and `implementchange[2]`.
- [ ] 6.14 Add tests proving non-verified tasks have no verifier metadata.
- [ ] 6.15 Add an example workflow YAML demonstrating a verified Agent task.

## 7. Verification

- [ ] 7.1 Run orchestrator unit tests with `cargo test`.
- [ ] 7.2 Run worker tests if task payload or TypeScript model types are updated.
- [ ] 7.3 Run `openspec status --change add-bounded-workflow-loops` and confirm all artifacts are complete.
