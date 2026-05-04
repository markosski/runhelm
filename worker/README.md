# RunHelm Worker

A containerized task executor for the RunHelm workflow orchestrator. The worker receives task payloads over **stdin** (one JSON object per line), executes them, and writes results back to **stdout** as JSON.

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

This runs the TypeScript source directly via `ts-node` without a build step.

You can pipe task payloads to it manually for quick testing:

```bash
echo '{"task": {"id": "abc123", "kind": {"Agent": {"model_id": "openai/gpt-4o-mini", "provider_url": "", "prompt": "say hi!"}}, "input_schemas": [], "output_schema": {"type": "object", "properties": {"response": {"type": "string"}}}, "expected_side_effects": [], "required_credentials": ["llm_api_key"]}, "inputs": []}' | npm run dev
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
