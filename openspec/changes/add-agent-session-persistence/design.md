## Context

RunHelm Agent tasks currently create a fresh worker-side agent execution for each attempt. Verifier retries and human-input resumes preserve workflow lineage in the orchestrator, but the worker reconstructs conversational context by injecting upstream inputs, verifier feedback, previous output, or human responses into a new prompt. That keeps the orchestrator deterministic, but it makes the engine responsible for more prompt-history management than it should own.

Pi already supports persistent JSONL sessions through `SessionManager`, including creating, opening, listing, and continuing sessions. RunHelm should use that capability behind a RunHelm-owned session storage boundary so Agent tasks can resume the same conversation across attempts, worker restarts, and future distributed workers.

## Goals / Non-Goals

**Goals:**
- Persist Agent conversation sessions durably enough that later attempts can load the same session.
- Add an Agent task `reuse_session` setting that defaults to `true`.
- Derive stable session keys from workflow instance and task identity rather than storing opaque session IDs on every task attempt.
- Propagate attempt `generation_index` through the executor payload boundary so workers can apply session conventions without explicit session policy metadata.
- Let Agent retry/resume attempts append only the new event prompt, such as human input or verifier feedback, instead of reinjecting full history.
- Keep workflow truth in the orchestrator: attempt lineage, task status, satisfaction, budgets, input mappings, and current waiting questions remain structured orchestration state.
- Log clearly when an expected reusable session cannot be loaded and allow the Agent attempt to continue with a fresh session.
- Start with a file-backed session store and leave room for cloud blob-backed storage.

**Non-Goals:**
- Replace workflow attempt metadata with Pi session files.
- Make Function or API call tasks session-aware.
- Implement a full cross-provider session format independent of Pi.
- Add UI for browsing or editing Agent sessions.
- Solve concurrent execution of the same Agent session from multiple workers beyond rejecting or serializing conflicting use.

## Decisions

### Add a RunHelm session storage boundary

Introduce a worker-side session storage abstraction for Agent sessions. The initial implementation should be file-backed and compatible with Pi `SessionManager` persistent session files. The abstraction should expose operations such open/load, persist, and resolve a stable session key to a local path usable by Pi.

The session key passed to the worker should not assume that the worker always has the same local filesystem path. Use a logical RunHelm session key with enough information for the worker to resolve it through the configured store.

For reusable Agent sessions, the session key is derived by convention from:

```text
workflow_instance_id + task_def_id
```

The key is scoped to the logical task, not the materialized attempt, so retries and resumes for the same Agent task reuse the same durable conversation.

Alternative considered: store raw Pi session JSONL in orchestrator task metadata. That would simplify retrieval but would bloat workflow instances, couple core state to Pi internals, and make every task status read carry conversation history.

### Control reuse through Agent task configuration

Add `reuse_session` to Agent task definitions. The field defaults to `true`.

When `reuse_session` is `true`, all materialized attempts for the same workflow instance and logical Agent task use the same durable session key. The worker derives that key from the workflow instance ID and logical task ID. The first attempt creates the session if it is missing. Later continuation attempts, such as human-input or verifier-feedback attempts, try to load the same session and log a clear diagnostic before creating a fresh replacement if the stored session is missing or unreadable.

When `reuse_session` is `false`, attempts do not reuse the logical task session. The implementation may use an attempt-scoped durable session key for observability or run the Agent without durable reuse, but it must not load previous conversation history for the logical task.

For ask and verifier interactions, the orchestrator should continue creating materialized attempts with explicit causes, previous attempt IDs, generation indexes, and budget metadata. The convention-derived session key is the continuity handle, not the source of truth for orchestration decisions.

This change only introduces the session-handling contracts needed by human-input-created Agent attempts. The public API and service flow that accepts human input, materializes the continuation attempt, and resumes the workflow is intentionally left to a separate human-input change. When that later flow creates a continuation attempt, it must preserve the same workflow instance ID and logical task ID so the worker derives the same reusable session key.

Alternative considered: persist a worker-returned opaque session reference on every task attempt. That makes the handle explicit in workflow state, but it adds metadata churn and carry-forward logic when the required continuity is already expressible as workflow instance plus logical task identity.

### Append event prompts instead of replaying full history

The worker Agent executor should distinguish the first execution from continuation executions:

```text
initial attempt:
  derive session key
  create session when missing
  prompt with task prompt and resolved upstream inputs

human-input attempt:
  derive same session key when reuse_session is true
  if the session exists, load it and append the submitted human response as the next session event
  if the session is missing or unreadable, log it, create a fresh session, and prompt with the full task prompt, resolved upstream inputs, and submitted human response

verifier-feedback attempt:
  derive same session key when reuse_session is true
  if the session exists, load it and append verifier feedback as the next session event
  if the session is missing or unreadable, log it, create a fresh session, and prompt with the full task prompt, resolved inputs or previous output, and verifier feedback
```

The original task prompt, previous user/assistant turns, tool calls, and prior corrections should already be in the session when a durable session is loaded. The orchestrator may still send structured event metadata for audit and executor behavior, but it should not synthesize a complete conversation transcript as the normal loaded-session path. When a session is unavailable, the worker rebuilds enough current context for that attempt instead of relying on prior session history.

Alternative considered: keep injecting ordered ask and verifier history into every attempt. That is provider-neutral and easy to inspect, but it duplicates session functionality, increases prompt size, and makes the engine responsible for conversational memory.

### Treat missing sessions as recoverable degradation

If a continuation attempt with `reuse_session` enabled expects an existing session and the worker cannot load it from durable storage, the worker should log a clear diagnostic including the logical session key and create a fresh session. Session loss should be rare, and a fresh attempt must still proceed using the full task prompt plus structured workflow inputs, verifier feedback, or human response data that the orchestrator sends for the current attempt. If the lost conversation context matters, the verifier can reject the result and drive another attempt.

The system can later add stricter policies that fail continuation attempts when a session is unavailable, but that policy can be derived from attempt generation (`generation_index > 1`) and does not require an explicit `require_existing_session` payload field.

Alternative considered: fail continuation attempts whenever loading fails. That makes session loss highly visible, but it adds failure coupling for a rare storage problem and is not required for the convention-derived key design.

### Keep storage implementation pluggable

The first storage adapter should support local file-backed sessions, suitable for single-host development and workers sharing a mounted directory. The API should leave room for blob-backed storage where a worker downloads a session file before execution and uploads the updated file after execution.

Workers should write updates atomically where possible. Blob-backed implementations should use generation IDs, ETags, or equivalent compare-and-swap semantics to avoid overwriting another worker's update.

Alternative considered: depend directly on Pi's default `~/.pi/agent/sessions` layout. That is useful for prototyping, but RunHelm needs explicit configuration, test isolation, and a path to remote storage.

### Preserve strict ownership boundaries

The orchestrator should own:
- workflow instance state
- task attempt lifecycle and satisfaction
- attempt cause and lineage
- ask/verifier budgets
- current `InputNeeded` description
- Agent `reuse_session` policy
- attempt generation metadata in execution payloads and convention-derived session key metadata in status reports

The worker and session store should own:
- convention-derived Agent session key resolution from workflow instance ID and task ID
- Pi session file loading and persistence
- conversation messages
- tool call history
- compaction entries
- provider/model-specific session details

This keeps the core engine observable and deterministic while allowing Agent implementations to use richer native session capabilities.

## Risks / Trade-offs

- [Risk] Session files become unavailable or corrupted -> Mitigation: log the session key and recover with a fresh session; verifier feedback can reject weak outputs and drive another attempt.
- [Risk] Local file-backed storage does not work for distributed workers -> Mitigation: define the storage boundary before implementation and treat local files as the first adapter, not the only design.
- [Risk] Concurrent attempts mutate the same session -> Mitigation: serialize attempts for a logical Agent session initially, and require optimistic concurrency controls for blob-backed storage.
- [Risk] Session content can contain sensitive user data and tool outputs -> Mitigation: keep session storage configurable, document retention expectations, and avoid copying session contents into broad workflow status APIs.
- [Risk] Pi session format changes -> Mitigation: isolate Pi-specific file handling in the worker adapter and expose only RunHelm-owned session keys through orchestrator APIs.
- [Risk] Debugging becomes harder if feedback is only in the session -> Mitigation: keep structured human-response and verifier-feedback events in task metadata or related orchestration state, but do not replay them as full prompt history by default.

## Migration Plan

1. Add `reuse_session` to Agent task definitions with a default of `true`.
2. Add task attempt generation metadata to executor payload models.
3. Add a file-backed Agent session store configuration for workers.
4. Update `AgentExecutor` to create a persistent session for initial reusable Agent attempts.
5. Update continuation attempts to load the existing session and append only the new human-input or verifier-feedback prompt event.
6. Add clear diagnostics and fresh-session recovery for missing or unreadable continuation sessions.
7. Add tests for default reuse behavior, session key derivation, continuation loading, opt-out behavior, and missing-session recovery.
8. Later, add a blob-backed session store adapter without changing orchestrator task semantics.

Rollback before launch can disable durable session reuse and return Agent execution to fresh-session prompt reconstruction. Once workflows rely on session continuity for correctness, rollback requires either preserving the session store or adding an explicit reconstruction path from structured events.

## Open Questions

- Should `reuse_session = false` use attempt-scoped durable sessions for observability, or in-memory sessions with no durable persistence?
- What exact serialized session key format should RunHelm use for file-backed and future blob-backed sessions?
- What retention and cleanup policy should apply to session files after workflow completion?
