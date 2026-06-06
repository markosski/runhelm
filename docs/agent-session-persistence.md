# Agent Session Persistence

Worker agent sessions can be stored as JSONL text through the file-backed session store.

Execution payloads carry `execution_metadata.generation_index` for the current task attempt. Agent executors derive logical session keys in the worker from `workflow_inst_id` and the logical task ID, while the Agent task definition carries the `reuse_session` policy.

Function and API call tasks do not receive Agent session keys, transcript contents, or worker-local session paths. They continue to use the shared task execution payload without depending on Agent session storage.

Session stores return `null` for missing sessions and throw typed `SessionStoreError` values for unreadable sessions. Agent executors can log the session key from those load outcomes and continue with a fresh session.

The default worker file session store writes complete session documents under `$HOME/.cache/runhelm/file_session_store`. This keeps Agent session persistence worker-local and writable in container deployments where `$HOME/.runhelm` is mounted read-only for credentials. Session keys are serialized from `workflow_instance_id` and `task_id`, then encoded into single `.jsonl` filenames, so logical keys never create nested paths or expose worker-local paths.

The worker materializes loaded JSONL into transient Pi session files under `$HOME/.cache/runhelm/temp_session` and creates fresh Pi native sessions under `$HOME/.cache/runhelm/native_session`. These paths are worker-local implementation details; task payloads, task results, and orchestrator state use logical RunHelm session keys rather than raw filesystem paths.

This storage is intended to persist agent session data across attempts handled by the same live worker container. It is not durable storage: session files can disappear when the container or its filesystem is removed.
