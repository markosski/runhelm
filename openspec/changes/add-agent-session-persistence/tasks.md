## 1. Core Models And Contracts

- [x] 1.1 Add `reuse_session` to Agent task definitions with a default of `true`.
- [x] 1.2 Add a core Agent session key type derived from workflow instance ID and task identity that is stable across workers and does not expose worker-local paths.
- [ ] 1.3 Extend execution metadata or executor payload models so Agent attempts receive the derived session key, reuse policy, and whether an existing session is required.
- [ ] 1.4 Ensure the executor result model does not need to return opaque session IDs for the normal convention-derived reuse path.
- [ ] 1.5 Add a clear session-load failure representation that can be propagated from worker execution to task failure.

## 2. Orchestrator Behavior

- [ ] 2.1 Derive reusable Agent session keys from `workflow_instance_id` and logical `task_def_id`.
- [ ] 2.2 Dispatch human-input-created Agent attempts with the same logical-task session key when `reuse_session` is true.
- [ ] 2.3 Dispatch verifier-feedback-created Agent attempts with the same logical-task session key when `reuse_session` is true.
- [ ] 2.4 Ensure Function and API call attempts do not require or receive Agent session metadata.
- [ ] 2.5 Treat session-load failures as task execution failures and mark the workflow failed.
- [ ] 2.6 Keep downstream binding resolution based on completed and satisfied task attempts, not session transcript contents.
- [ ] 2.7 Expose derived session key metadata in status/result reporting without exposing session transcripts.
- [ ] 2.8 Ensure `reuse_session = false` does not reuse the logical-task session across attempts.

## 3. Worker Session Store

- [x] 3.1 Add a worker-side Agent session store interface for create, load, and persist operations.
- [x] 3.2 Implement a file-backed Agent session store compatible with Pi persistent session files.
- [ ] 3.3 Add worker configuration for the file-backed session storage location.
- [ ] 3.4 Resolve stable RunHelm session keys to worker-local Pi session files without exposing raw worker-local paths.
- [ ] 3.5 Add missing-session and unreadable-session errors that include the failed session key.

## 4. Agent Executor Integration

- [ ] 4.1 Update `AgentExecutor` to create a durable session for initial reusable Agent attempts when the derived session key is missing.
- [ ] 4.2 Update `AgentExecutor` to load an existing session before continuation attempts.
- [ ] 4.3 Prompt initial sessions with the task prompt and resolved upstream inputs.
- [ ] 4.4 Prompt human-input continuation attempts with the submitted human response as the next session event.
- [ ] 4.5 Prompt verifier-feedback continuation attempts with verifier feedback as the next session event.
- [ ] 4.6 Stop reinjecting complete prior ask or verifier feedback history when a durable session is loaded.
- [ ] 4.7 Ensure required continuation sessions fail clearly instead of creating blank replacement sessions.
- [ ] 4.8 Persist the updated session after successful, failed, or input-needed Agent execution when a session was opened.

## 5. Tests

- [ ] 5.1 Add core model serialization tests for `reuse_session` defaulting to true and explicit false values.
- [ ] 5.2 Add orchestrator tests proving reusable Agent attempts derive the same logical-task session key.
- [ ] 5.3 Add orchestrator tests proving human-input attempts dispatch the same logical-task session key when `reuse_session` is true.
- [ ] 5.4 Add orchestrator tests proving verifier-feedback attempts dispatch the same logical-task session key when `reuse_session` is true.
- [ ] 5.5 Add orchestrator tests proving session-load failures fail the task and workflow without satisfying downstream bindings.
- [ ] 5.6 Add worker session store tests for file-backed create, load, persist, and missing-session behavior.
- [ ] 5.7 Add Agent executor tests for initial session creation and continuation session loading.
- [ ] 5.8 Add Agent executor tests proving a missing required continuation session does not create a blank session.
- [ ] 5.9 Add status/result reporting tests proving derived session keys are visible but transcripts are not.
- [ ] 5.10 Add tests proving `reuse_session = false` does not load prior logical-task session history.

## 6. Documentation

- [ ] 6.1 Document durable Agent session persistence under `docs/`, including the distinction between workflow state and Agent conversation state.
- [ ] 6.2 Document file-backed worker session store configuration.
- [ ] 6.3 Document missing-session failure behavior and why blank continuation sessions are rejected.
- [ ] 6.4 Update existing docs that describe ask or verifier feedback history injection to explain durable session continuation.
- [ ] 6.5 Document future blob-backed session store expectations without requiring that adapter in this change.
- [ ] 6.6 Document `reuse_session`, including its default `true` behavior and opt-out semantics.
