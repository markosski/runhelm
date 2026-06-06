# Agent Session Persistence

Worker agent sessions can be stored as JSONL text through the file-backed session store.

Execution payloads carry `execution_metadata.generation_index` for the current task attempt. Agent executors derive logical session keys in the worker from `workflow_inst_id` and the logical task ID, while the Agent task definition carries the `reuse_session` policy.

Function and API call tasks do not receive Agent session keys, transcript contents, or worker-local session paths. They continue to use the shared task execution payload without depending on Agent session storage.

Session stores return `null` for missing sessions and throw typed `SessionStoreError` values for unreadable sessions. Agent executors can log the session key from those load outcomes and continue with a fresh session.

The default worker file session store writes complete session documents under `$HOME/.runhelm/agent_sessions`. Session keys are encoded into single `.jsonl` filenames, so keys such as `workflow_instance_id/task_id` do not create nested paths or expose worker-local paths.

The file-backed adapter also exposes an atomic string-file writer for local JSONL materialization, such as preparing a runtime session file for an Agent session manager before persisting the updated JSONL back through `SessionStore`.

This storage is intended to persist agent session data across attempts handled by the same live worker container. It is not durable storage: session files can disappear when the container or its filesystem is removed.
