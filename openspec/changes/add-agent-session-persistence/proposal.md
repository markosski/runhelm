## Why

Agent retries currently depend on reconstructing context through injected prompts and execution metadata. Durable agent sessions let RunHelm preserve conversational continuity across human-input resumes, verifier-guided retries, worker restarts, and future distributed worker deployments without turning the orchestrator into a prompt-history manager.

## What Changes

- Add durable agent session persistence for Agent task execution.
- Introduce a session storage boundary that can support local file-backed storage initially and cloud blob-backed storage later.
- Add `reuse_session` to Agent task definitions, defaulting to `true`.
- Derive stable Agent session keys from workflow instance and task identity so workers can load or create the correct session before execution without storing opaque session IDs on every attempt.
- Change Agent task execution so retry/resume attempts append new prompt events, such as human responses or verifier feedback, to the existing session instead of reinjecting complete feedback history.
- Preserve orchestrator-owned workflow truth for attempt lineage, budgets, statuses, and audit metadata while delegating conversational continuity to the agent session.
- Fail clearly when a required durable session cannot be loaded, rather than silently continuing from a blank session.

## Capabilities

### New Capabilities

- `agent-session-persistence`: Defines durable session storage and resume semantics for Agent task execution.

### Modified Capabilities

- `task-executor`: Extend the execution contract so Agent executions can receive convention-derived session keys and session-related failures.
- `workflow-dataflow-engine`: Derive Agent session keys from workflow instance and logical task identity, honor `reuse_session`, and preserve deterministic workflow behavior when Agent attempts are resumed or retried.

## Impact

- Orchestrator core models for Agent task configuration and execution metadata.
- Workflow engine behavior for materializing Agent attempts that should reuse an existing session.
- Worker task payload and result types shared between orchestrator and worker.
- Worker `AgentExecutor` session lifecycle, including load/create/persist behavior.
- Pi session integration, likely through `SessionManager` file-backed sessions first.
- Future storage adapters for local filesystem and cloud blob-backed session persistence.
- Tests for session key derivation, `reuse_session` defaults, missing-session failures, and Agent retry/resume behavior.
