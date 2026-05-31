## 1. Model And Validation

- [ ] 1.1 Add `AskControlConfig` with `max_attempts` and `TaskControl.ask` to core workflow models.
- [ ] 1.2 Validate `control.ask.max_attempts` is greater than zero.
- [ ] 1.3 Remove `ask` from `TaskTypeDef::Agent` and update all constructors, fixtures, and serde expectations.
- [ ] 1.4 Update workflow definition validation to accept `control.ask` only for Agent tasks.
- [ ] 1.5 Add validation errors for Function and API call tasks that declare `control.ask`.

## 2. Attempt Context And Reporting

- [ ] 2.1 Add attempt cause metadata that can distinguish initial, human-input, and verifier-feedback attempts.
- [ ] 2.2 Add ask state/history metadata with ask attempt number, max attempts, previous task attempt ID, input request descriptions, and human responses.
- [ ] 2.3 Record previous task attempt ID and human input data on human-input-created attempts.
- [ ] 2.4 Extend execution metadata so ask-created attempts receive ordered ask history.
- [ ] 2.5 Extend task result metadata serialization to include ask attempt lineage and budget metadata.
- [ ] 2.6 Report `TaskStatus::InputNeeded` distinctly from `Running` in task result and workflow status APIs.
- [ ] 2.7 Include the input request description in status/result responses for waiting task attempts.

## 3. Engine Behavior

- [ ] 3.1 Gate `ExecutionResult::InputNeeded` so it succeeds only for tasks with `control.ask`.
- [ ] 3.2 Fail task execution with a clear error when a task without `control.ask` returns `InputNeeded`.
- [ ] 3.3 Preserve an `InputNeeded` attempt without output and without downstream satisfaction.
- [ ] 3.4 Add an internal materialization helper that creates the next local attempt for the same logical task while ask budget remains.
- [ ] 3.5 Fail clearly without creating another attempt when submitted human input would exceed `control.ask.max_attempts`.
- [ ] 3.6 Pass submitted human input and ordered ask history to the new task attempt through execution metadata.
- [ ] 3.7 Ensure completed human-input attempts can satisfy downstream bindings while prior `InputNeeded` attempts cannot.
- [ ] 3.8 Ensure ask-created and verifier-created attempts record distinct causes while preserving exact `input_mapping`.

## 4. WorkflowService And API

- [ ] 4.1 Add a WorkflowService method that accepts workflow instance ID, waiting task attempt ID, and human response.
- [ ] 4.2 Validate in WorkflowService that the target task attempt exists, is `InputNeeded`, is ask-enabled, and has remaining ask budget.
- [ ] 4.3 Have WorkflowService append the human response to ask history and materialize the next task attempt.
- [ ] 4.4 Have WorkflowService return the newly materialized task attempt ID.
- [ ] 4.5 Add an HTTP API route for submitting human input to a waiting task attempt.
- [ ] 4.6 Ensure the API delegates orchestration decisions to WorkflowService and returns clear not-found, invalid-state, and exhausted-budget errors.

## 5. Tests

- [ ] 5.1 Add workflow registration tests for Agent `control.ask`, Function `control.ask`, and API call `control.ask`.
- [ ] 5.2 Add workflow registration tests for zero `control.ask.max_attempts`.
- [ ] 5.3 Add workflow registration tests proving Agent-kind ask configuration is rejected after the model replacement.
- [ ] 5.4 Add engine tests for ask-enabled `InputNeeded` status and workflow `InputNeeded` status.
- [ ] 5.5 Add engine tests for non-ask tasks returning `InputNeeded` and failing clearly.
- [ ] 5.6 Add WorkflowService tests for human input creating `task[n+1]` while preserving the original `InputNeeded` attempt.
- [ ] 5.7 Add WorkflowService tests for invalid human input submissions to unknown, non-waiting, or exhausted attempts.
- [ ] 5.8 Add API tests for human input submission success and error responses.
- [ ] 5.9 Add tests for ask history being provided to later ask-created attempts.
- [ ] 5.10 Add tests for ask budget exhaustion failing clearly without creating an extra attempt.
- [ ] 5.11 Add tests proving downstream tasks consume the completed human-input attempt rather than the waiting attempt.
- [ ] 5.12 Add status/result API tests proving `InputNeeded` and ask lineage metadata are visible.

## 6. Documentation

- [ ] 6.1 Update docs under `docs/` to describe `control.ask` as Agent-only workflow control with finite `max_attempts`.
- [ ] 6.2 Document how `InputNeeded` attempts, human input responses, ask history, and follow-up task attempts are represented.
- [ ] 6.3 Document the WorkflowService and API path used to provide human input and unblock a task.
- [ ] 6.4 Document how ask-created attempts differ from verifier-feedback-created attempts.
- [ ] 6.5 Update examples or diagrams that still show ask configured inside the Agent task kind.
