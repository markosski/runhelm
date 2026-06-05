# Agent Session Persistence

Worker agent sessions can be stored as JSONL text through the file-backed session store.

The default worker file session store writes complete session documents under `$HOME/.runhelm/agent_sessions`. Session keys are encoded into single `.jsonl` filenames, so keys such as `workflow_instance_id/task_id` do not create nested paths or expose worker-local paths.

This storage is intended to persist agent session data across attempts handled by the same live worker container. It is not durable storage: session files can disappear when the container or its filesystem is removed.
