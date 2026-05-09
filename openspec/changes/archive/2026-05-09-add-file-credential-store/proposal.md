## Why

Worker credentials are currently configured with hardcoded development fallbacks in code, which risks leaking secrets and makes deployment-specific credential changes require code edits. The worker needs a simple file-backed credential store that keeps credentials out of source while preserving the existing credential lookup port used by task executors.

## What Changes

- Add a worker credential store implementation that reads credentials from `~/.runhelm/file_credentials.json`, containing a flat key/value object.
- Configure worker startup to use the file-backed credential store instead of hardcoded credential defaults.
- Add validation and clear startup errors for missing, unreadable, invalid, or incorrectly shaped credential files.
- Keep credential lookup behavior unchanged for executors: credentials are resolved by name through the existing `CredentialsPort`.
- Document the credential file location, JSON structure, and read-only `~/.runhelm` container mount.

## Capabilities

### New Capabilities
- `worker-credential-store`: Defines worker credential loading from a flat JSON file and credential lookup behavior for task executors.

### Modified Capabilities

## Impact

- Affected code:
  - `worker/src/core/ports/CredentialsPort.ts`
  - `worker/src/adapters/*CredentialsAdapter.ts`
  - `worker/src/index.ts`
  - `worker/README.md`
- The worker reads credentials from `~/.runhelm/file_credentials.json` by default.
- Container deployments should mount `~/.runhelm` read-only for the worker.
- Existing task executor code should not need behavior changes because it already depends on the credential port.
- No new runtime service dependency is required; the implementation uses local filesystem access and JSON parsing.
