# Capability: docker-executor

## Purpose
Defines the behavior of `DockerExecutor` — an implementation of `ExecutorPort` that dispatches task execution into a Docker container, injects a structured invocation payload via stdin, and collects the task output from stdout.

## Requirements

### Requirement: Task Invocation Payload
`DockerExecutor` SHALL construct a `TaskInvocationPayload` envelope combining the full `TaskDef` and the resolved inputs array, and write it as a single JSON object to the container's stdin. This gives the container everything it needs to perform its work without any out-of-band configuration.

The payload shape is:
```json
{
  "task": { <full TaskDef serialized as JSON> },
  "inputs": [ <resolved input values in index order> ]
}
```

#### Scenario: Payload written to stdin
- **WHEN** `DockerExecutor::execute` is called with a `TaskDef` and a resolved input slice
- **THEN** the executor SHALL serialize `{ "task": <TaskDef>, "inputs": <inputs> }` as a single JSON object and write it to the container's stdin before the container begins reading

#### Scenario: Empty inputs in payload
- **WHEN** `DockerExecutor::execute` is called with an empty input slice
- **THEN** the `inputs` field in the payload SHALL be `[]` (an empty JSON array)

#### Scenario: Stdin closed after write
- **WHEN** the payload has been fully written
- **THEN** the executor SHALL close the stdin stream so the container can detect EOF and begin processing

### Requirement: Task Output Envelope
Container images SHALL write a single `TaskExecutionResult` JSON object to stdout before exiting. This envelope represents either a successful output or a task-level error, allowing the container to signal failure with structured context rather than relying solely on exit codes.

The envelope shape is:
```json
// Success
{ "status": "ok", "output": { <task output JSON> } }

// Task-level error
{ "status": "error", "message": "<human-readable description>", "code": "<optional machine-readable error code>" }
```

The `code` field is optional. The `output` field is required when `status` is `"ok"`. The `message` field is required when `status` is `"error"`.

#### Scenario: Successful task output
- **WHEN** the container completes its work successfully
- **THEN** it SHALL write `{ "status": "ok", "output": <result> }` to stdout and exit with code 0

#### Scenario: Task-level error
- **WHEN** the container encounters a business-level failure (e.g., upstream API returned an error, agent failed to produce output)
- **THEN** it SHALL write `{ "status": "error", "message": "...", "code": "..." }` to stdout and exit with code 1

### Requirement: Output Envelope Parsing
`DockerExecutor` SHALL parse the `TaskExecutionResult` envelope from stdout and map it to the `ExecutorPort` return type. It SHALL distinguish between task-level errors (structured envelope with `status: "error"`) and infrastructure-level errors (non-zero exit with no valid envelope).

#### Scenario: Envelope with status ok
- **WHEN** stdout contains a valid `TaskExecutionResult` with `status: "ok"`
- **THEN** `DockerExecutor::execute` SHALL return `Ok` with the value of the `output` field

#### Scenario: Envelope with status error
- **WHEN** stdout contains a valid `TaskExecutionResult` with `status: "error"`
- **THEN** `DockerExecutor::execute` SHALL return `Err` containing the envelope's `message` (and `code` if present), regardless of exit code

#### Scenario: Unparseable stdout (infrastructure failure)
- **WHEN** the container exits and stdout does not contain a valid `TaskExecutionResult` JSON object
- **THEN** `DockerExecutor::execute` SHALL return `Err` describing the parse failure, including the raw stdout and stderr content

### Requirement: Non-Zero Exit Code Handling
`DockerExecutor` SHALL treat any non-zero container exit code as an execution failure.

#### Scenario: Non-zero exit
- **WHEN** the container exits with a non-zero exit code
- **THEN** `DockerExecutor::execute` SHALL return `Err` containing the exit code and any content from stderr

### Requirement: Container Lifecycle Management
`DockerExecutor` SHALL create, start, wait for, and remove the container for each task execution. Containers SHALL be removed after output collection regardless of exit status.

#### Scenario: Container removed after success
- **WHEN** the container exits with code 0 and output is collected
- **THEN** `DockerExecutor` SHALL remove the container before returning `Ok`

#### Scenario: Container removed after failure
- **WHEN** the container exits with a non-zero exit code or encounters an error
- **THEN** `DockerExecutor` SHALL attempt to remove the container before returning `Err`

### Requirement: Image Resolution
`DockerExecutor` SHALL resolve the Docker image to run from a constructor-supplied image map keyed by task kind name (`"ApiCall"` or `"Agent"`).

#### Scenario: Known task kind
- **WHEN** the task's `kind` variant maps to an entry in the image map
- **THEN** `DockerExecutor` SHALL use the corresponding image name to create the container

#### Scenario: Unknown task kind
- **WHEN** the task's `kind` variant has no entry in the image map
- **THEN** `DockerExecutor::execute` SHALL return `Err` with a message stating that no image is configured for that task kind

### Requirement: Stderr Capture for Diagnostics
`DockerExecutor` SHALL capture the container's stderr output and include it in error messages when execution fails, to aid debugging.

#### Scenario: Stderr included in error
- **WHEN** the container exits with a non-zero code and has written to stderr
- **THEN** the returned `Err` SHALL include the stderr content as part of its message

#### Scenario: Stderr ignored on success
- **WHEN** the container exits with code 0
- **THEN** stderr content SHALL NOT affect the return value (it may be silently discarded or logged)

### Requirement: No Image Auto-Pull
`DockerExecutor` SHALL NOT attempt to pull Docker images automatically. If the required image is not present on the local Docker daemon, container creation SHALL fail with an error from the Docker API.

#### Scenario: Image not present locally
- **WHEN** the resolved image is not available on the local Docker daemon
- **THEN** `DockerExecutor::execute` SHALL return `Err` propagating the Docker API error, without attempting a pull
