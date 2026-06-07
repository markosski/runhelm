## Why

Agent retries currently depend on reconstructing context through injected prompts and execution metadata. Durable agent sessions let RunHelm preserve conversational continuity across human-input resumes, verifier-guided retries, worker restarts, and future distributed worker deployments without turning the orchestrator into a prompt-history manager.

## What Changes

- Add durable agent session persistence for Agent task execution.
- Introduce a session storage boundary that can support local file-backed storage initially and cloud blob-backed storage later.
- Add `reuse_session` to Agent task definitions, defaulting to `true`.
- Derive stable Agent session keys from workflow instance and task identity so workers can load or create the correct session before execution without storing opaque session IDs on every attempt.
- Change Agent task execution so retry/resume attempts append new prompt events, such as human responses or verifier feedback, to the existing session instead of reinjecting complete feedback history.
- Preserve orchestrator-owned workflow truth for attempt lineage, budgets, statuses, and audit metadata while delegating conversational continuity to the agent session.
- Log clearly when an expected durable session cannot be loaded, then recover with a fresh session so verifier/dataflow behavior can determine whether another attempt is needed.

## Capabilities

### New Capabilities

- `agent-session-persistence`: Defines durable session storage and resume semantics for Agent task execution.

### Modified Capabilities

- `task-executor`: Extend the execution contract with task attempt generation metadata while workers derive convention-based Agent session keys.
- `workflow-dataflow-engine`: Provide stable workflow/task identity and generation metadata, honor `reuse_session`, and preserve deterministic workflow behavior when Agent attempts are resumed or retried.

## Impact

- Orchestrator core models for Agent task configuration and execution metadata.
- Workflow engine behavior for materializing Agent attempts that should reuse an existing session.
- Worker task payload and result types shared between orchestrator and worker.
- Worker `AgentExecutor` session lifecycle, including load/create/persist behavior.
- Pi session integration, likely through `SessionManager` file-backed sessions first.
- Future storage adapters for local filesystem and cloud blob-backed session persistence.
- Tests for session key derivation, `reuse_session` defaults, missing-session recovery, and Agent retry/resume behavior.

## Out of Scope Clarification

This change prepares Agent session handling for human-input-created continuation attempts, but it does not complete the public human-input submission API or the full end-to-end resume flow. A later human-input change will create and verify the API path that materializes the continuation attempt and supplies the submitted human response.
