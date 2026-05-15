# RunHelm Worker

The worker executes tasks for the RunHelm orchestrator. It starts as a resident Node.js process, registers its capabilities, asks the orchestrator for work, executes claimed tasks, and sends task results back.

Workers can communicate with the orchestrator over a Unix domain socket or over HTTP. The orchestrator owns both endpoints; workers only initiate connections and requests.

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

By default the worker connects to the socket at `RUNHELM_SOCKET_PATH`, or `/tmp/runhelm.sock` when the environment variable is not set. Set `RUNHELM_WORKER_TRANSPORT=http` to use the orchestrator HTTP API instead.

The worker reads credentials from `~/.runhelm/file_credentials.json` during startup. The file must contain a flat JSON object whose keys are credential names and whose values are strings:

```json
{
  "llm_api_key": "example-llm-key",
  "system_brave_api_key": "example-brave-key"
}
```

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

## Worker HTTP Protocol

HTTP workers use the same task and result payloads as the IPC protocol. The orchestrator listens on port `3000` by default.

### `POST /workers/register`

Registers a worker.

```json
{
  "worker_id": "remote-worker-1",
  "capabilities": ["Agent", "ApiCall", "Function"]
}
```

Response:

```json
{
  "type": "registration_ack",
  "worker_id": "remote-worker-1"
}
```

### `POST /workers/tasks/claim`

Long-polls for one task. Returns `no_task` when the poll timeout expires.

```json
{
  "worker_id": "remote-worker-1"
}
```

Task response:

```json
{
  "type": "task_dispatch",
  "task_id": "summarize_user-0",
  "task": {},
  "inputs": []
}
```

Empty response:

```json
{
  "type": "no_task"
}
```

### `POST /workers/tasks/{task_id}/result`

Completes a claimed task.

```json
{
  "kind": "success",
  "output": {
    "response": "hello world"
  }
}
```

## Worker IPC Protocol

The worker and orchestrator exchange newline-delimited JSON over the Unix socket. The worker registers once, then sends `task_request` whenever it is ready for more work.

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

### Task Request

Sent by the worker when it is ready to claim work:

```json
{
  "type": "task_request"
}
```

### Task Dispatch

Sent by the orchestrator in response to `task_request`:

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

### No Task

Sent by the orchestrator when a task request times out without pending work:

```json
{
  "type": "no_task"
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
    tools: ["_all_"]
    ask: false
    schema_failure_retry_times: 0
required_credentials:
  - llm_api_key
```

Use `tools: []` to disable tools, `tools: ["_all_"]` to allow every tool available to the worker, or list specific tool names such as `["fetch_url", "get_current_time"]`.

Agent tools include RunHelm built-ins, Pi coding-agent built-ins, and Pi-compatible extension tools. The Pi built-in tool names are `read`, `bash`, `edit`, and `write`.

The worker uses Pi's resource loader, so TypeScript extensions and skills are supported. Extension and skill packages are runtime resources, not worker application dependencies. They must already be installed in the worker image or mounted into the worker environment before startup. Packages installed under the worker's `node_modules` are auto-discovered when their `package.json` contains a `pi` manifest:

```json
{
  "name": "@acme/runhelm-tools",
  "pi": {
    "extensions": ["./extensions"],
    "skills": ["./skills"]
  }
}
```

Extension tools are enabled with the `name` passed to `pi.registerTool(...)`. Skills are added to the agent system prompt using Pi's skill formatter; when a skill is relevant, the agent can use the `read` tool to load the full `SKILL.md`, so include `read` or `"_all_"` in `tools` for skill-driven tasks.

You can also provide comma-separated extension files, directories, or package roots with `RUNHELM_AGENT_EXTENSION_PATHS`.

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
| `RUNHELM_WORKER_TRANSPORT` | `ipc` | Worker transport. Use `ipc` for the Unix socket path or `http` for remote polling. |
| `RUNHELM_SOCKET_PATH` | `/tmp/runhelm.sock` | Unix socket path used by the worker and orchestrator IPC server. |
| `RUNHELM_ORCHESTRATOR_HTTP_URL` | `http://127.0.0.1:3000` | Orchestrator base URL used when `RUNHELM_WORKER_TRANSPORT=http`. |
| `WORKER_ID` | hostname plus process id | Worker id sent during registration. |
| `RUNHELM_FUNCTION_TIMEOUT_MS` | `300000` | Timeout for Function dependency install and Function execution. |
| `RUNHELM_AGENT_EXTENSION_PATHS` | unset | Comma-separated Pi extension files, directories, or package roots to load in addition to auto-discovered installed Pi packages. Relative paths are resolved from the worker process cwd. |
| `RUNHELM_PI_AGENT_DIR` | `$HOME/.pi/agent` | Pi resource-loader agent directory used for user-level extension discovery metadata. |

Credential values are not read from environment variables. Put credential values in `~/.runhelm/file_credentials.json`.

## Docker

Build the worker image:

```bash
docker build -t runhelm-worker worker
```

The worker image installs Pi resource packages separately from `worker/package.json`. By default it includes `@ogulcancelik/pi-web-browse@1.0.6` for the example agent workflow. Override the image-only package list with:

```bash
docker build \
  --build-arg RUNHELM_PI_PACKAGES="@ogulcancelik/pi-web-browse@1.0.6 @acme/runhelm-tools@1.2.3" \
  -t runhelm-worker worker
```

Use an empty build arg to produce an image with no extra Pi packages:

```bash
docker build --build-arg RUNHELM_PI_PACKAGES= -t runhelm-worker worker
```

Run the worker container with access to the orchestrator socket:

```bash
docker run --rm \
  -e RUNHELM_SOCKET_PATH=/tmp/runhelm.sock \
  -v /tmp/runhelm.sock:/tmp/runhelm.sock \
  -v ~/.runhelm:/home/runhelm/.runhelm:ro \
  runhelm-worker
```

Mount `~/.runhelm` read-only in containers. It must contain `file_credentials.json`. Keep runtime files such as `runhelm.sock` outside this directory; the default socket path remains `/tmp/runhelm.sock`.

## Project Structure

```text
worker/
├── src/
│   ├── adapters/
│   │   ├── FileCredentialsAdapter.ts
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
