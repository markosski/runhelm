# RunHelm Worker

The worker executes tasks for the RunHelm orchestrator. It starts as a resident Node.js process, connects to the orchestrator over a Unix domain socket, registers its capabilities, receives task dispatch messages, and sends task results back over the same socket.

The worker does not expose HTTP endpoints. HTTP requests go to the orchestrator API.

## Requirements

- Node.js 20+
- npm
- Docker, if building the worker image

## Development

Install dependencies:

```bash
npm install
```

Build TypeScript:

```bash
npm run build
```

Run the worker from compiled output:

```bash
npm start
```

Run the worker from TypeScript source:

```bash
npm run dev
```

The worker connects to the socket at `RUNHELM_SOCKET_PATH`, or `/tmp/runhelm.sock` when the environment variable is not set. The orchestrator owns that socket and must be running before a worker can connect.

## Orchestrator HTTP Endpoints

The orchestrator listens on port `3000` by default.

### `GET /health`

Health check.

Response:

```text
OK
```

### `POST /workflow-def`

Registers a workflow definition.

Example:

```bash
yq -o=json worker/example_workflow.yaml \
  | curl -sS -X POST http://localhost:3000/workflow-def \
      -H 'content-type: application/json' \
      --data-binary @-
```

Response:

```json
{
  "status": "created",
  "id": "simple-function-workflow"
}
```

### `POST /workflow-def/{def_id}`

Creates and starts a workflow instance from a registered workflow definition. The current handler accepts a JSON body but does not read fields from it.

Example:

```bash
curl -sS -X POST http://localhost:3000/workflow-def/simple-function-workflow \
  -H 'content-type: application/json' \
  -d '{}'
```

Response:

```json
{
  "status": "created",
  "id": "simple-function-workflow-1780000000000000000"
}
```

### `POST /workflow-def/{def_id}/tasks/{task_id}`

Executes a task from a registered workflow definition in isolation. This bypasses workflow instance creation and is intended for testing individual tasks.

Example:

```bash
curl -sS -X POST http://localhost:3000/workflow-def/simple-function-workflow/tasks/summarize_user \
  -H 'content-type: application/json' \
  -d '{ "inputs": [] }'
```

Response:

```json
{
  "status": "success",
  "output": {
    "response": "hello world"
  }
}
```

### `GET /workflows/{id}`

Returns a workflow instance status report.

Example:

```bash
curl -sS http://localhost:3000/workflows/simple-function-workflow-1780000000000000000
```

Response shape:

```json
{
  "instance_id": "simple-function-workflow-1780000000000000000",
  "workflow_def_id": "simple-function-workflow",
  "status": "Completed",
  "tasks": [
    {
      "task_id": "summarize_user",
      "status": "Completed",
      "has_output": true
    }
  ]
}
```

### `GET /workflows/{workflow_instance_id}/tasks/{task_id}`

Returns a task result for a workflow instance.

Example:

```bash
curl -sS http://localhost:3000/workflows/simple-function-workflow-1780000000000000000/tasks/summarize_user
```

Example response:

```json
{
  "response": "hello world"
}
```

Unknown routes return `404`.

## Worker IPC Protocol

The worker and orchestrator exchange newline-delimited JSON over the Unix socket.

### Worker Registration

Sent by the worker after connecting:

```json
{
  "type": "register",
  "worker_id": "host-12345",
  "capabilities": ["Agent", "ApiCall", "Function"]
}
```

### Registration Ack

Sent by the orchestrator:

```json
{
  "type": "registration_ack",
  "worker_id": "host-12345"
}
```

### Task Dispatch

Sent by the orchestrator:

```json
{
  "type": "task_dispatch",
  "task_id": "summarize_user",
  "task": {
    "id": "summarize_user",
    "kind": {
      "Function": {
        "dependencies": [],
        "code": "export default async function run(ctx) { return { response: 'hello world' }; }"
      }
    },
    "input_schemas": [],
    "output_schema": {
      "type": "object",
      "required": ["response"],
      "properties": {
        "response": { "type": "string" }
      }
    },
    "expected_side_effects": [],
    "required_credentials": []
  },
  "inputs": []
}
```

### Task Result

Sent by the worker:

```json
{
  "type": "task_result",
  "task_id": "summarize_user",
  "result": {
    "kind": "success",
    "output": {
      "response": "hello world"
    }
  }
}
```

Failure result:

```json
{
  "type": "task_result",
  "task_id": "summarize_user",
  "result": {
    "kind": "failure",
    "reason": "error message"
  }
}
```

Input-needed result:

```json
{
  "type": "task_result",
  "task_id": "agent_task",
  "result": {
    "kind": "input_needed",
    "description": "question for the user"
  }
}
```

## Supported Task Types

### Function

Runs JavaScript ESM in a per-task temporary directory and executes it in a child Node.js process. The task must export a default async or sync function.

```yaml
kind:
  Function:
    dependencies:
      - name: left-pad
        version: 1.3.0
    code: |
      import leftPad from "left-pad";

      export default async function run(ctx) {
        return {
          response: leftPad(ctx.inputs[0].value, 5, "0")
        };
      }
```

The Function context contains:

```ts
{
  inputs: unknown[];
  credentials: Record<string, string>;
}
```

`dependencies` is required. Use `[]` when the task has no npm dependencies.

### Agent

Runs an LLM-backed agent task.

Required task fields:

```yaml
kind:
  Agent:
    model_id: "google/gemini-2.5-flash"
    provider_url: ""
    prompt: "Return a JSON response."
    tools: []
    ask: false
    schema_failure_retry_times: 0
required_credentials:
  - llm_api_key
```

### ApiCall

Currently simulated by the worker.

```yaml
kind:
  ApiCall:
    url: "http://localhost:3000/health"
    method: "GET"
```

## Configuration

| Variable | Default | Description |
| --- | --- | --- |
| `RUNHELM_SOCKET_PATH` | `/tmp/runhelm.sock` | Unix socket path used by the worker and orchestrator IPC server. |
| `WORKER_ID` | hostname plus process id | Worker id sent during registration. |
| `RUNHELM_FUNCTION_TIMEOUT_MS` | `300000` | Timeout for Function dependency install and Function execution. |
| `LLM_API_KEY` | development fallback in code | Credential value exposed as `llm_api_key`. |
| `BRAVE_API_KEY` | development fallback in code | Credential value exposed as `system_brave_api_key`. |

## Docker

Build the worker image:

```bash
docker build -t runhelm-worker worker
```

Run the worker container with access to the orchestrator socket:

```bash
docker run --rm \
  -e RUNHELM_SOCKET_PATH=/tmp/runhelm.sock \
  -v /tmp/runhelm.sock:/tmp/runhelm.sock \
  runhelm-worker
```

## Project Structure

```text
worker/
├── src/
│   ├── adapters/
│   │   ├── executors/
│   │   │   ├── AgentExecutor.ts
│   │   │   ├── ApiCallExecutor.ts
│   │   │   └── FunctionExecutor.ts
│   │   └── InMemoryCredentialsAdapter.ts
│   ├── core/
│   │   ├── models/
│   │   │   └── TaskDef.ts
│   │   └── ports/
│   └── index.ts
├── Dockerfile
├── example_workflow.yaml
├── example_workflow_agent.yaml
├── package.json
└── tsconfig.json
```
