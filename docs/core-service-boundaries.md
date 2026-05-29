# Core Service Boundaries

RunHelm's orchestrator core separates queue/execution coordination, workflow lifecycle operations, workflow execution, and side-effect adapters into distinct services.

## Orchestrator

`Orchestrator` coordinates runtime execution operations:

- queueing, dequeuing, and scheduling workflow execution
- delegating workflow execution to `WorkflowEngine`
- reconciling active workflow state during startup
- executing workflow tasks in isolation for debugging and tests

It should stay thin when an operation is primarily workflow persistence, validation, or read-model formatting.

## WorkflowEngine

`WorkflowEngine` is the workflow execution state machine. Given a persisted workflow instance, it advances execution by:

- materializing task attempts
- resolving task inputs from data bindings
- validating task inputs and outputs
- invoking `ExecutorPort`
- updating task, verifier, and workflow statuses
- persisting execution progress through `StoragePort`

Execution rules, verifier loop progression, and workflow status transitions belong here.

## WorkflowService

`WorkflowService` owns workflow persistence use cases and API-facing read models:

- validating, normalizing, and registering workflow definitions
- retrieving workflow definitions
- creating workflow instances from registered workflow definitions
- listing workflow summaries
- resolving task result requests to materialized task attempts
- listing materialized task results
- converting task instances into `TaskResult` responses

Workflow CRUD and read behavior that formats persisted workflow state without advancing execution belongs here.

## FunctionService

`FunctionService` owns reusable function definition persistence use cases:

- registering reusable function definitions
- deleting reusable function definitions

Function registry behavior belongs here instead of in `WorkflowService`, because functions are shared resources that workflows may reference.

## Ports

Side effects remain behind ports:

- `StoragePort` persists workflow definitions, function definitions, and workflow instances.
- `ExecutorPort` executes one task attempt.
- `WorkflowQueuePort` stores pending workflow instance IDs.

Core services depend on these ports rather than concrete adapters so storage, execution, and queue implementations remain swappable.

## API Models

API-specific request and response DTOs live under `api/models.rs`. Core models should represent persisted workflow state, task definitions, execution metadata, and verifier domain state. Status reports, workflow list summaries, and queue responses are API read models and should not be added to `core/models.rs`.
