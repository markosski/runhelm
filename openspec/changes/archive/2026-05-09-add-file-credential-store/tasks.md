## 1. Credential Adapter

- [x] 1.1 Add `FileCredentialsAdapter` implementing `CredentialsPort` in `worker/src/adapters/`.
- [x] 1.2 Parse the configured JSON file as a flat object of string credential values.
- [x] 1.3 Reject missing paths, unreadable files, invalid JSON, non-object top-level values, and non-string credential values with clear errors.
- [x] 1.4 Ensure adapter errors never include credential values.

## 2. Worker Startup Integration

- [x] 2.1 Resolve the default credential file path to `~/.runhelm/file_credentials.json` during worker startup.
- [x] 2.2 Replace hardcoded `InMemoryCredentialsAdapter` construction in `worker/src/index.ts` with the file-backed adapter.
- [x] 2.3 Update local types so `processTask` accepts the `CredentialsPort` interface instead of a concrete in-memory adapter.
- [x] 2.4 Ensure startup fails before IPC registration when credential configuration is invalid.
- [x] 2.5 Keep IPC socket configuration separate from the credential directory and continue using `RUNHELM_SOCKET_PATH`/`/tmp/runhelm.sock`.

## 3. Documentation

- [x] 3.1 Update `worker/README.md` configuration docs for `~/.runhelm/file_credentials.json`.
- [x] 3.2 Document the required flat JSON structure with an example.
- [x] 3.3 Document mounting `~/.runhelm` read-only for containerized workers.
- [x] 3.4 Remove documentation for hardcoded credential fallback defaults.

## 4. Verification

- [x] 4.1 Add coverage for successful credential lookup from a JSON file.
- [x] 4.2 Add coverage for missing, invalid, and incorrectly shaped credential files.
- [x] 4.3 Run `npm run build` in `worker/` and any available worker test command.
