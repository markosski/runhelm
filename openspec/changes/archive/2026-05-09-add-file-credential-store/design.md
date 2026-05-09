## Context

The TypeScript worker already resolves credentials through `CredentialsPort`, and executors call `getCredential(name)` without depending on how credentials are stored. Worker startup currently constructs `InMemoryCredentialsAdapter` with environment variables and hardcoded development fallback values for `llm_api_key` and `system_brave_api_key`.

The change should keep executor-facing behavior stable while moving runtime credentials out of source code. The target credential format is intentionally simple: a local JSON file at `~/.runhelm/file_credentials.json` whose top-level value is an object mapping credential names to string values.

## Goals / Non-Goals

**Goals:**
- Provide a `CredentialsPort` implementation backed by a local JSON file.
- Remove hardcoded credential fallback values from worker startup.
- Fail fast when the configured credential file cannot be read or parsed.
- Keep the JSON structure simple enough for local development and container secret mounts.
- Use `~/.runhelm` as the credential mount directory and mount it read-only in containers.
- Avoid logging credential values.
- Document worker configuration and file format.

**Non-Goals:**
- Secret rotation without worker restart.
- Encrypted credential files or external secret managers.
- Nested credential documents, non-string values, or provider-specific credential schemas.
- Changes to task definitions or executor credential lookup APIs.

## Decisions

### File-backed adapter implements the existing port

Add `FileCredentialsAdapter` under `worker/src/adapters/` that implements `CredentialsPort`. It loads a JSON object from disk and exposes `getCredential(name)` by looking up a key in an internal map.

Alternative considered: extending `InMemoryCredentialsAdapter` to accept a file path. Keeping file I/O in a separate adapter preserves the current in-memory implementation for tests and small fixtures while isolating parsing and validation logic.

### Load and validate once during worker startup

Worker startup will read the credential file before connecting to the orchestrator. Startup fails if the configured file path is missing, unreadable, invalid JSON, not a top-level object, or contains non-string values.

Alternative considered: lazy loading on each credential lookup. That would allow editing the file while the worker runs, but it adds repeated I/O to task execution and makes failures occur after the worker has already registered. Loading once is simpler, deterministic, and enough for the first file-backed store.

### Use `~/.runhelm/file_credentials.json` as the default credential file

Worker startup will read credentials from `~/.runhelm/file_credentials.json`. Container deployments should mount the host or secret-provided `~/.runhelm` directory into the worker container as read-only. The worker will not provide built-in secret defaults.

Alternative considered: keeping only a `RUNHELM_CREDENTIALS_FILE` path. A fixed default path reduces required configuration and gives local and container usage one documented convention. An environment override can still be added later if multiple credential profiles become necessary.

### Keep the IPC socket outside the credential directory

The orchestrator IPC socket should continue to default to `/tmp/runhelm.sock` and remain configurable through `RUNHELM_SOCKET_PATH`. The credential directory is read-only secret/config material, while the socket is writable runtime state created by the orchestrator and consumed by workers.

Alternative considered: placing `runhelm.sock` under `~/.runhelm`. That conflicts with the read-only mount requirement and mixes secret material with ephemeral IPC state. If a non-`/tmp` location becomes necessary later, introduce a separate runtime directory such as `RUNHELM_RUNTIME_DIR` or use `XDG_RUNTIME_DIR`, rather than reusing the credential mount.

### Validate as flat string key/value JSON

The adapter accepts only a top-level JSON object whose keys are credential names and whose values are strings.

Example:

```json
{
  "llm_api_key": "example-llm-key",
  "system_brave_api_key": "example-brave-key"
}
```

Alternative considered: accepting arbitrary JSON values and coercing them to strings. Rejecting non-string values avoids surprising credential values such as `[object Object]`, `true`, or numbers.

## Risks / Trade-offs

- Missing file blocks worker startup -> Document `~/.runhelm/file_credentials.json` and fail with a clear error that names the path but not credential values.
- File changes require worker restart -> Accept for this initial implementation; add reload semantics later if needed.
- Plaintext local file can be mishandled -> Document that the file should be provided through deployment secret mechanisms and excluded from source control.
- Read-only credential mount cannot host runtime files -> Keep the IPC socket at `/tmp/runhelm.sock` by default and configurable via `RUNHELM_SOCKET_PATH`.
- Existing local workflows that relied on hardcoded fallbacks will stop working -> Provide README migration guidance with a sample JSON file.

## Migration Plan

1. Add the file-backed adapter and unit coverage for successful lookup plus invalid file shapes.
2. Update worker startup to construct the file-backed adapter from `~/.runhelm/file_credentials.json`.
3. Remove hardcoded credential fallback values from `worker/src/index.ts`.
4. Update worker documentation with the default credential path, read-only mount guidance, and sample JSON.
5. For local development, create an untracked credentials JSON file at `~/.runhelm/file_credentials.json` before starting the worker.

Rollback is to restore worker startup to use an in-memory adapter populated from environment variables. No task or orchestrator data migration is required.

## Open Questions

- Should a future change introduce `RUNHELM_CREDENTIALS_FILE` for multiple local credential profiles, or is the fixed `~/.runhelm/file_credentials.json` convention enough for now?
