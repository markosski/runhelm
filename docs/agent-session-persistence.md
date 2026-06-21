# Agent Session Persistence

RunHelm Agent tasks can persist Pi conversation sessions as JSONL through the worker session store. The persisted session is Agent conversation state: user and assistant messages, tool calls, compaction entries, and provider-specific session details. It does not replace workflow state.

The orchestrator remains the source of truth for workflow instance status, task attempt status, satisfaction, generation indexes, verifier budgets, input mappings, and `InputNeeded` questions. Downstream data binding still reads completed and satisfied task attempts, not Agent session transcripts.

## Session Reuse

Agent task definitions support `reuse_session`. When the field is omitted, RunHelm treats it as `true`.

When `reuse_session` is `true`, the worker derives one logical session key from the workflow instance ID and the logical task ID. All materialized attempts for that logical Agent task use that same key, so a retry or resume can continue the previous Agent conversation.

When `reuse_session` is `false`, later attempts must not load prior logical-task conversation history. This is useful when the Agent should behave as a fresh evaluator for every attempt. Agent verifier tasks should usually set `reuse_session: false` because a verifier should evaluate the current upstream output cleanly instead of carrying its own prior verifier conversation across iterations.

Function and API call tasks do not receive Agent session keys, transcript contents, or worker-local session paths. They continue to use the shared task execution payload without depending on Agent session storage.

## Continuation Behavior

Execution payloads carry `execution_metadata.generation_index` for the current task attempt. The worker uses that attempt number with the task's `reuse_session` policy to decide whether to load an existing session.

For an initial reusable Agent attempt, the worker creates a session and prompts the Agent with the task prompt and resolved upstream inputs.

For a continuation attempt with a loaded session, the worker appends only the current event context. Examples include a submitted human response or the latest verifier feedback for the task being regenerated. The original task prompt, previous assistant turns, prior tool results, and prior corrections are expected to already be in the loaded session.

For a continuation attempt where no session can be loaded, the worker creates a fresh replacement session and rebuilds enough context from structured workflow data: the task prompt, upstream inputs, previous output or prior feedback history when available, and the current human response or verifier feedback.

This change prepares the worker-side session handling needed for human-input-created continuation attempts. The public human-input submission API and full end-to-end resume flow are completed separately.

## Missing Session Recovery

Session stores return `null` for missing sessions and throw typed `SessionStoreError` values for unreadable sessions. Agent executors log the session key and attempt number for both cases, then continue with a fresh session.

This is a recoverable degradation rather than an immediate task failure. Session loss should be rare, and the fresh attempt still receives the full current task context. If lost conversation context causes a weak output, normal verifier behavior can reject the output and create another attempt with explicit feedback.

The trade-off is that missing-session recovery can consume verifier iterations instead of failing fast. A future stricter policy could fail continuation attempts when a required session is unavailable, but the current convention can derive that expectation from `generation_index > 1` without adding a separate payload field.

## File-Backed Store

The default worker file session store writes complete JSONL session documents under:

```text
$HOME/.cache/runhelm/file_session_store
```

This keeps Agent session persistence worker-local and writable in container deployments where `$HOME/.runhelm` may be mounted read-only for credentials. The store is intentionally scoped to the worker container or host filesystem. It persists sessions across attempts handled by the same live worker container, but it is not durable storage after the container or its filesystem is removed.

Session keys are serialized from `workflow_instance_id` and `task_id`, then encoded into single `.jsonl` filenames. Logical keys never create nested paths and worker-local paths are not exposed through task payloads, task results, or orchestrator state.

The worker materializes loaded JSONL into transient Pi session files under:

```text
$HOME/.cache/runhelm/temp_session
```

Fresh Pi native sessions are created under:

```text
$HOME/.cache/runhelm/native_session
```

Those paths are worker-local implementation details. The RunHelm session store boundary owns conversion between stable RunHelm session keys and Pi session files.

Agent execution keeps Pi's SDK-managed stream function so model authentication continues through Pi `AuthStorage` and `ModelRegistry`. RunHelm task credentials are injected as runtime API-key overrides for the selected model provider before the session is created.

## Future Blob Store

The session store interface is intended to allow a future blob-backed implementation without changing workflow attempt semantics. A blob store should use the same logical RunHelm session keys, download or materialize a Pi-compatible JSONL file before execution, and persist the updated JSONL after execution.

Blob-backed implementations should add concurrency protection, such as ETags, generation IDs, or compare-and-swap writes, so two workers do not silently overwrite the same logical session. The orchestrator task model should not need to store opaque Pi session IDs or transcript contents when that adapter is added.

## Remote Worker Pinning

The OpenSpec change `add-workspace-session-persistence` defines the planned interaction between Agent sessions and remote worker placement. Workers must be configured with `RUNHELM_WORKER_HOST_ID`; RunHelm does not auto-detect this identity. The value should identify the durable execution state domain that owns the workspace and session roots, not the worker container or process.

Every workflow instance is pinned to a registered worker host when the workflow instance is created for execution, and reusable Agent sessions in that workflow instance continue on the pinned host. This keeps host-local session files and workspace files aligned. Multiple worker processes may share the same `RUNHELM_WORKER_HOST_ID` when they share the same durable workspace and session roots.

Worker container restart does not by itself imply session loss. A replacement worker can resume work for the pinned host when it registers or renews via heartbeat with the same `RUNHELM_WORKER_HOST_ID` and has access to the same session store root.

If no worker is currently registered for the pinned host, RunHelm should wait rather than silently continuing a reusable Agent session on another host. If the host remains unavailable past the host-loss policy, RunHelm should fail the pinned workflow instance. Default retry keeps the same pinned host. A force retry may reassign the workflow to another registered host, but that explicitly accepts that host-local Agent session and workspace context may be lost.
