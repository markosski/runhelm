## ADDED Requirements

### Requirement: File-backed credential loading
The worker SHALL support a credential store implementation that loads credentials from `~/.runhelm/file_credentials.json`.

#### Scenario: Worker loads credentials from default file
- **WHEN** `~/.runhelm/file_credentials.json` exists and contains a flat object of string keys and string values
- **THEN** the worker SHALL make each key available through `CredentialsPort.getCredential`

#### Scenario: Worker startup fails without credential file
- **WHEN** the worker starts and `~/.runhelm/file_credentials.json` does not exist
- **THEN** the worker SHALL fail startup with a clear credential file error

#### Scenario: Worker startup fails for unreadable file
- **WHEN** `~/.runhelm/file_credentials.json` exists but cannot be read
- **THEN** the worker SHALL fail startup with an error that identifies the credential file path

### Requirement: Read-only credential mount
Containerized worker deployments SHALL mount `~/.runhelm` as a read-only directory containing `file_credentials.json`.

#### Scenario: Worker container receives credential mount
- **WHEN** the worker is run in a container
- **THEN** the deployment SHALL mount `~/.runhelm` read-only and provide `file_credentials.json` inside that directory

#### Scenario: Runtime state is not written to credential mount
- **WHEN** the worker or orchestrator needs runtime IPC files such as sockets
- **THEN** those files SHALL NOT be created under the read-only `~/.runhelm` credential mount

### Requirement: Credential file format validation
The credential file MUST contain valid JSON whose top-level value is an object mapping credential names to string values.

#### Scenario: Invalid JSON is rejected
- **WHEN** the credential file does not contain valid JSON
- **THEN** the worker SHALL fail startup with a credential file parse error

#### Scenario: Non-object JSON is rejected
- **WHEN** the credential file top-level JSON value is not an object
- **THEN** the worker SHALL fail startup with a credential file format error

#### Scenario: Non-string credential value is rejected
- **WHEN** any credential file property has a non-string value
- **THEN** the worker SHALL fail startup with a credential file format error identifying the credential name

### Requirement: Credential lookup behavior
The file-backed credential store SHALL implement the same lookup semantics as `CredentialsPort`: return the string value for a known credential name and `undefined` for an unknown credential name.

#### Scenario: Known credential is requested
- **WHEN** an executor requests a credential name present in the credential file
- **THEN** `getCredential` SHALL resolve to that credential string value

#### Scenario: Unknown credential is requested
- **WHEN** an executor requests a credential name absent from the credential file
- **THEN** `getCredential` SHALL resolve to `undefined`

### Requirement: No hardcoded worker secrets
Worker startup MUST NOT define hardcoded credential values or development fallback secrets in source code.

#### Scenario: Worker credentials are configured externally
- **WHEN** the worker process starts
- **THEN** credential values SHALL come from `~/.runhelm/file_credentials.json` rather than source-code literals

#### Scenario: Credential errors avoid secret disclosure
- **WHEN** worker startup reports a credential file error
- **THEN** the error message SHALL NOT include any credential value from the file
