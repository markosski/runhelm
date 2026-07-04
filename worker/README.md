# RunHelm Worker

The worker executes tasks for the RunHelm orchestrator. It starts as a resident Node.js process, registers with the orchestrator, asks for work, executes claimed tasks, and sends task results back.

Workers communicate with the orchestrator over HTTP. The orchestrator owns the HTTP API; workers only initiate requests.

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

By default the worker connects to the orchestrator worker API at `http://127.0.0.1:3001`. Set `RUNHELM_ORCHESTRATOR_HTTP_URL` when the worker API is reachable at a different URL.

Set `RUNHELM_WORKER_HOST_ID` before starting the worker. This value is required and should identify the durable host state domain that owns the worker's local workspace and session stores, not a short-lived worker process or container ID.

The worker registers with the orchestrator before polling for tasks. If the orchestrator service name or worker API is not reachable yet during container startup, registration is retried until it succeeds. These startup retries are expected during Compose bootstrap and do not require a worker restart.

The worker reads credentials from `~/.runhelm/file_credentials.json` during startup. The file must contain a flat JSON object whose keys are credential names and whose values are strings:

```json
{
  "llm_api_key": "example-llm-key",
  "system_brave_api_key": "example-brave-key"
}
```

## Orchestrator HTTP Endpoints

The public orchestrator API listens on port `3000` by default.

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
yq -o=json worker/examples/example_workflow.yaml \
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

### `POST /function-def`

Registers a reusable function definition. Workflow Function tasks can reference the registered definition with `ref`.

Example:

```bash
curl -sS -X POST http://localhost:3000/function-def \
  -H 'content-type: application/json' \
  --data-binary @functions/mailgun-dispatcher/dist/mailgun.fetch_inbound_mail.json
```

Response:

```json
{
  "status": "created",
  "id": "mailgun.fetch_inbound_mail",
  "version": null
}
```

### `DELETE /function-def/{def_id}`

Deletes a reusable function definition.

Example:

```bash
curl -sS -X DELETE http://localhost:3000/function-def/mailgun.fetch_inbound_mail
```

### `POST /workflow-def/{def_id}`

Creates and starts a workflow instance from a registered workflow definition.
The JSON body is used as the initial input for root tasks that declare
`input_schemas`. The request body is persisted as a single JSON value and then
passed as one input slot to eligible root tasks. `null` is treated as no initial
input.

Example:

```bash
curl -sS -X POST http://localhost:3000/workflow-def/simple-function-workflow \
  -H 'content-type: application/json' \
  -d '{"name":"Ada"}'
```

Response:

```json
{
  "status": "queued",
  "id": "simple-function-workflow-1780000000000000000",
  "pinned_host_id": "local-host"
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

### `GET /workflows/{workflow_instance_id}/tasks`

Returns all materialized task attempt results for a workflow instance.

Example:

```bash
curl -sS http://localhost:3000/workflows/simple-function-workflow-1780000000000000000/tasks
```

Example response:

```json
{
  "workflow_instance_id": "simple-function-workflow-1780000000000000000",
  "tasks": [
    {
      "task_id": "summarize_user[1]",
      "result": {
        "status": "success",
        "input": [
          {
            "name": "Ada"
          }
        ],
        "output": {
          "response": "hello world"
        },
        "requested_task_id": "summarize_user[1]",
        "resolved_attempt_id": "summarize_user[1]"
      }
    }
  ]
}
```

### `GET /workflows/{workflow_instance_id}/tasks/{task_id}`

Returns a task result for a workflow instance.

Task result responses include `status`, `input`, and status-specific fields such as `output` or `error_message`. Attempt metadata fields such as `task_def_id`, `task_attempt_id`, `generation_index`, `input_mapping`, `satisfaction`, and `verifier_metadata` are included only when the resolved task attempt needs that context.

Example:

```bash
curl -sS http://localhost:3000/workflows/simple-function-workflow-1780000000000000000/tasks/summarize_user
```

Example response:

```json
{
  "status": "success",
  "input": [
    {
      "name": "Ada"
    }
  ],
  "output": {
    "response": "hello world"
  },
  "requested_task_id": "summarize_user",
  "resolved_attempt_id": "summarize_user[1]"
}
```

### `GET /workflow-queue`

Returns pending workflow instance IDs waiting for scheduler execution.

Example:

```bash
curl -sS http://localhost:3000/workflow-queue
```

Response shape:

```json
{
  "pending": [
    "simple-function-workflow-1780000000000000000"
  ]
}
```

### `DELETE /workflow-queue/{id}`

Removes one pending workflow instance from the scheduler queue without deleting the workflow instance record.

Example:

```bash
curl -sS -X DELETE http://localhost:3000/workflow-queue/simple-function-workflow-1780000000000000000
```

### `DELETE /workflow-queue`

Purges all pending workflow instances from the scheduler queue without deleting workflow instance records.

Example:

```bash
curl -sS -X DELETE http://localhost:3000/workflow-queue
```

Unknown routes return `404`.

## Worker HTTP Protocol

Workers use HTTP JSON endpoints for registration, task claiming, and task completion. The worker-only orchestrator API listens on `127.0.0.1:3001` by default.

### `POST /workers/register`

Registers a worker.

```json
{
  "worker_id": "remote-worker-1",
  "host_id": "local-dev-host"
}
```

Response:

```json
{
  "type": "registration_ack",
  "worker_id": "remote-worker-1",
  "heartbeat_interval_ms": 5000
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
  "workflow_inst_id": "workflow-1",
  "task": {},
  "workspace_path_suffix": "workflow-1/taskid-summarize_user",
  "inputs": []
}
```

The worker resolves `workspace_path_suffix` under its own `RUNHELM_WORKSPACE_ROOT`, creates the directory, updates `.timestamp`, and passes that worker-local absolute path to the task executor.

Empty response:

```json
{
  "type": "no_task"
}
```

### `POST /workers/heartbeat`

Joins or renews a worker registration using the worker process ID and stable host ID.

```json
{
  "worker_id": "remote-worker-1",
  "host_id": "local-dev-host"
}
```

Response:

```json
{
  "status": "accepted",
  "worker_id": "remote-worker-1"
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

## Supported Task Types

Every task may set `timeout_secs`. When omitted, the orchestrator falls back to `RUNHELM_TASK_TIMEOUT_SECS`.

### Function

Runs JavaScript ESM in a per-task temporary directory and executes it in a child Node.js process. The task must export a default async or sync function.

Functions may be declared inline for small tasks:

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

Or referenced from a registered function definition:

```yaml
kind:
  Function:
    ref: mailgun.fetch_inbound_mail
```

Register reusable functions separately:

```bash
(cd functions/mailgun-dispatcher && npm run build)

curl -sS -X POST http://localhost:3000/function-def \
  -H 'content-type: application/json' \
  --data-binary @functions/mailgun-dispatcher/dist/mailgun.fetch_inbound_mail.json
```

The build writes YAML registration artifacts for review and JSON artifacts for the current HTTP API.

Delete a registered function with:

```bash
curl -sS -X DELETE http://localhost:3000/function-def/mailgun.fetch_inbound_mail
```

Use a task-specific timeout when execution duration differs from the global default:

```yaml
id: summarize_user
timeout_secs: 60
kind:
  Function:
    dependencies: []
    code: |
      export default async function run() {
        return { response: "hello world" };
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
    skills: []
    ask: false
    schema_failure_retry_times: 0
required_credentials:
  - llm_api_key
```

Use `tools: []` to disable tools, `tools: ["_all_"]` to allow every tool available to the worker, or list specific tool names such as `["fetch_url", "get_current_time"]`.

Agent tools include RunHelm built-ins, Pi coding-agent built-ins, and Pi-compatible extension tools. The Pi built-in tool names are `read`, `bash`, `edit`, and `write`.

Set `ask: true` and include `ask_user` in `tools` when an Agent may pause for human input. See `worker/examples/example_human_input_workflow.yaml` for a minimal workflow that enters `InputNeeded`, then continues after `POST /workflows/{workflow_instance_id}/tasks/{task_id}/human-input`.

Use `skills: []` to expose no skills, or list exact skill names such as `["ticket-triage"]`. Skills do not support `"_all_"`.

The worker uses Pi's resource loader, so TypeScript extensions and skills are supported. Extension and skill packages are runtime resources, not worker application dependencies. They must already be installed in the worker image or mounted into the worker environment before startup. The default Docker Compose deployment mounts `${RUNHELM_SKILLS_DIR:-${HOME}/.runhelm/skills}` into `/home/runhelm/.pi/agent/skills` as read-only. If a mounted skill and an installed package skill have the same name, the mounted skill takes priority.

Packages installed under the worker's `node_modules` are auto-discovered when their `package.json` contains a `pi` manifest:

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
| `RUNHELM_ORCHESTRATOR_HTTP_URL` | `http://127.0.0.1:3001` | Worker API base URL used for worker registration, task claiming, and task completion. |
| `RUNHELM_WORKER_HOST_ID` | required | Stable host identity sent during registration. Use the same value for workers that share durable workspace and session roots. |
| `RUNHELM_WORKSPACE_ROOT` | `$HOME/.cache/runhelm/workspaces` | Root directory where dispatched `workspace_path_suffix` values are materialized before task execution. The Docker Compose worker sets this to `/workspaces`. |
| `WORKER_ID` | hostname plus process id | Worker id sent during registration. |
| `RUNHELM_FUNCTION_TIMEOUT_MS` | `300000` | Timeout for Function dependency install and Function execution. |
| `RUNHELM_TASK_TIMEOUT_SECS` | `300` | Orchestrator fallback timeout for tasks that do not set `timeout_secs`. |
| `RUNHELM_AGENT_EXTENSION_PATHS` | unset | Comma-separated Pi extension files, directories, or package roots to load in addition to auto-discovered installed Pi packages. Relative paths are resolved from the worker process cwd. |
| `RUNHELM_PI_AGENT_DIR` | `$HOME/.pi/agent` | Pi resource-loader agent directory used for user-level extension discovery metadata. |

Credential values are not read from environment variables. Put credential values in `~/.runhelm/file_credentials.json`.
Agent session JSONL files are stored under `$HOME/.cache/runhelm/file_session_store`. This worker-local cache is used to reuse Agent sessions across attempts handled by the same live worker container.

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

Run the worker container with access to the orchestrator worker API:

```bash
docker run --rm \
  -e RUNHELM_ORCHESTRATOR_HTTP_URL=http://host.docker.internal:3001 \
  -v ~/.runhelm:/home/runhelm/.runhelm:ro \
  runhelm-worker
```

Mount `~/.runhelm` read-only in containers. It must contain `file_credentials.json`.

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
├── examples/
│   ├── example_input.yaml
│   ├── example_mailgun_dispatcher_workflow.yaml
│   ├── example_workflow.yaml
│   ├── example_workflow_agent.yaml
│   └── example_workspace_download_workflow.yaml
├── package.json
└── tsconfig.json
```
