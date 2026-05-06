# RunHelm Worker

A containerized task executor for the RunHelm workflow orchestrator. The worker runs as a resident service listening on a **Unix Domain Socket** (`/tmp/worker.sock`), executes tasks, and returns results as JSON.

## Prerequisites

- [Node.js](https://nodejs.org/) v20+
- [Docker](https://www.docker.com/) (for building and running the container image)

## Development

### Install dependencies

```bash
npm install
```

### Run in development mode

```bash
npm run dev
```

This runs the TypeScript source directly via `ts-node` without a build step. It will start listening on `/tmp/worker.sock` by default.

### Manual Testing (JSON)

You can send JSON payloads to the socket using `socat`:

```bash
echo '{"task": {"id": "abc123", "kind": {"Agent": {"model_id": "google/gemini-2.5-flash", "prompt": "say hi!"}}, "input_schemas": [], "output_schema": {"type": "object", "properties": {"response": {"type": "string"}}}, "expected_side_effects": [], "required_credentials": ["llm_api_key"]}, "inputs": []}' | socat -t 60 - UNIX-CONNECT:/tmp/worker.sock,shut-none
```

### Manual Testing (YAML)

For a better developer experience, you can write tasks in YAML and use `yq` to pipe them to the worker:

1. Create a `task.yaml`:

```yaml
task:
  id: "abc123"
  kind:
    Agent:
      model_id: "google/gemini-2.5-flash"
      prompt: |-
        Collect information about following stocks: NVDA, TSLA, INTC, AMD.
        Organize information exactly as the following output:
        - NVDA - 10.00
        - TSLA - 10.00
        ...
  output_schema:
    type: "object"
    properties:
      response: { type: "string" }
  required_credentials: ["llm_api_key"]
inputs: []
```

2. Send to the worker:

```bash
# Using the Python yq wrapper (kislyuk/yq)
yq . task.yaml | socat -t 60 - UNIX-CONNECT:/tmp/worker.sock,shut-none

# Using the Go yq version (mikefarah/yq)
yq -o=json task.yaml | socat -t 60 - UNIX-CONNECT:/tmp/worker.sock,shut-none
```

### Build

Compile TypeScript to JavaScript in `dist/`:

```bash
npm build
```

### Run the compiled output

```bash
npm start
```

## Testing

```bash
npm test
```

> **Note:** No test suite is configured yet. Add your test framework (e.g. [Vitest](https://vitest.dev/)) and update the `test` script in `package.json`.

## stdin/stdout Protocol

The worker communicates via line-delimited JSON on **stdin/stdout**. Each line is one message.

**Input** (one JSON object per line on stdin):

```json
{
  "task": {
    "id": "abc123",
    "kind": {
      "Agent": {
        "model_id": "openai/gpt-4o-mini",
        "provider_url": "",
        "prompt": "say hi!"
      }
    },
    "input_schemas": [],
    "output_schema": {
      "type": "object",
      "properties": {
        "response": {
          "type": "string"
        }
      }
    },
    "expected_side_effects": [],
    "required_credentials": ["llm_api_key"]
  },
  "inputs": []
}
```

**Output** (written to stdout):

```json
{"status": "ok", "output": {"response": "Task executed successfully"}}
```

**Errors** (written to stdout as JSON, not stderr):

```json
{"status": "error", "message": "SyntaxError: Unexpected token ...", "code": null}
```

The worker exits cleanly when stdin is closed (EOF).

## Docker

### Build the image

```bash
docker build -t runhelm-worker .
```

The Dockerfile uses a two-stage build:
1. **Builder** — installs all dependencies and compiles TypeScript.
2. **Production** — copies only the compiled `dist/` and production dependencies, keeping the image lean.

### Run the container

The container reads tasks from stdin, so attach an interactive terminal or pipe input:

```bash
# Interactive (manual testing)
docker run -i runhelm-worker

# Pipe a task
echo '{"task": {"id": "abc123", "kind": {"Agent": {"model_id": "google/gemini-2.5-flash", "provider_url": null, "prompt": "hello?"}}, "input_schemas": [], "output_schema": {"type": "object", "properties": {"response": {"type": "string"}}}, "expected_side_effects": [], "required_credentials": ["llm_api_key"]}, "inputs": []}' | docker run -i runhelm-worker
```

## Project Structure

```
worker/
├── src/
│   ├── core/
│   │   └── ports/
│   │       └── CredentialsPort.ts    # Port interface for fetching credentials
│   ├── adapters/
│   │   └── EnvCredentialsAdapter.ts  # Example adapter fetching from environment
│   └── index.ts        # Entry point — reads stdin, dispatches tasks, writes stdout
├── Dockerfile           # Multi-stage Docker build
├── package.json
└── tsconfig.json
```
