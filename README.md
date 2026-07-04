# RunHelm

RunHelm is an agentic workflow orchestrator for teams that want to compose AI agents, API calls, and code execution into reliable multi-step runs.

The project separates control-plane concerns from execution concerns:

- The `orchestrator` owns workflow definitions, run state, scheduling, and status APIs.
- The `worker` executes individual tasks in an isolated runtime with typed inputs and outputs.
- The `frontend` is the start of an operator UI for workflows, runs, and system visibility.

## Why RunHelm

Most agent demos stop at "the model produced an answer." Real systems need more:

- structured workflows instead of one-off prompts
- explicit task dependencies and data flow
- resumable execution and observable run state
- typed contracts between steps
- pluggable execution backends and credentials

RunHelm is being built to provide that execution model. It treats an agent the same way it treats a function or API task: as a node in a workflow with declared inputs, outputs, and credentials.

## What A Workflow Looks Like

A workflow is defined as JSON and contains:

- tasks
- data bindings between task outputs and downstream task inputs
- optional per-task timeouts
- optional per-task input schemas and output schemas
- task kinds such as `Agent`, `ApiCall`, and `Function`

That definition becomes a workflow instance at runtime. The orchestrator tracks task state (`Pending`, `Running`, `InputNeeded`, `Completed`, `Failed`) and promotes the overall run state as work progresses.

## Architecture

```mermaid
flowchart TD
    User["Developer or<br>Product System"]
    UI["Frontend UI React + Vite"]
    API["Orchestrator API Axum"]
    App["Orchestrator Application Layer<br>Orchestrator"]
    Engine["Workflow Engine"]
    Storage["Storage Port<br>workflow defs + function defs + run state"]
    WorkflowQueue["Workflow Queue Port<br>pending workflow instances"]
    Scheduler["Workflow Scheduler"]
    Executor["Executor Port<br>DockerExecutor"]
    WorkerPool["WorkerPool<br>task dispatch queue + in-flight timeouts"]
    WorkerAPI["Worker HTTP API<br>register | claim | result"]
    Worker["Worker Runtime<br>TypeScript poller"]
    Credentials["Credentials Port<br>local file adapter"]
    Factory["ExecutorFactory"]
    AgentExec["AgentExecutor<br>Pi agent runtime"]
    ApiExec["ApiCallExecutor"]
    FunctionExec["FunctionExecutor<br>isolated Node.js child process"]
    Tools["Approved Agent Tools<br>HTTP | Fetch | Search | Time | Ask User | Coding/Pi extensions"]
    Skills["Pi Skills<br>loaded for approved agents"]
    Providers["LLM Providers / External APIs"]
    Npm["npm registry<br>function dependencies"]

    User --> UI
    User --> API
    UI --> API
    API --> App
    App --> Engine
    App --> WorkflowQueue
    Scheduler --> WorkflowQueue
    Scheduler --> Engine
    Engine --> Storage
    Engine --> Executor
    Executor --> WorkerPool
    API --> WorkerAPI
    WorkerAPI --> WorkerPool
    Worker -->|register / claim / post result| WorkerAPI
    Worker --> Credentials
    Worker --> Factory
    Factory --> AgentExec
    Factory --> ApiExec
    Factory --> FunctionExec
    AgentExec --> Tools
    AgentExec --> Skills
    AgentExec --> Providers
    ApiExec --> Providers
    FunctionExec --> Npm
    FunctionExec --> Providers

```

## Component Roles

### `orchestrator/`

Rust control plane built around ports and adapters:

- `src/core/engine.rs` runs workflow instances by finding runnable tasks, executing them, validating outputs, and propagating data bindings.
- `src/core/models.rs` defines workflow, task, run, and status models.
- `src/ports/` abstracts persistence and execution so storage backends and worker backends stay replaceable.
- `src/api/` exposes HTTP endpoints such as `/health`, `GET /orchestrator/status`, `POST /workflow-def`, `GET /workflows`, `GET /workflows?status=Running`, and `GET /workflows/:id`. See [Operational Status](docs/operational-status.md) for details on the status endpoint.

Current default wiring uses in-memory storage, an in-memory workflow queue, and a `DockerExecutor` backed by `WorkerPool`. The fake executor still exists for tests, but normal local startup now expects TypeScript workers to register, claim task dispatches over HTTP, and post results back to the orchestrator.

### `worker/`

TypeScript execution runtime for task payloads:

- claims task payloads from the orchestrator over HTTP
- selects the correct executor through `ExecutorFactory`
- supports agent, API-call, and function-style task execution
- validates task output against JSON Schema using `ajv`
- reads required credentials through a credentials port, currently the local file adapter

The agent executor already shows the intended shape of the system: provider-agnostic model selection, credential lookup behind a port, approved tool and skill selection, built-in retrieval/external-call tools, and Pi coding-agent resources loaded from local extensions.

### `frontend/`

React operator console prototype for:

- workflow visibility
- run monitoring
- operational status views

It is currently a UI shell rather than a fully integrated console, but it establishes the product direction for observing orchestrated runs.

## Execution Model

At a high level, RunHelm executes a workflow like this:

1. A workflow definition is registered with tasks and data bindings.
2. A workflow instance is created for a specific run.
3. The orchestrator identifies tasks whose dependencies are satisfied.
4. Runnable tasks are queued behind an executor backend.
5. Workers claim queued tasks over HTTP.
6. Task outputs are schema-validated and propagated to downstream tasks.
7. State is persisted after progress so runs can be inspected and resumed.

This design keeps orchestration logic deterministic while allowing execution environments to evolve independently.

## Design Principles

The repository is already aligned around a few architectural constraints:

- side effects live behind ports and adapters
- pure coordination logic stays in cohesive modules
- schemas can define contracts between tasks when a workflow needs runtime validation
- execution backends remain pluggable
- TypeScript is preferred for dynamic task execution, while Rust provides a strong orchestration core

## Current Status

RunHelm is in an early implementation stage. The important pieces already visible in the codebase are:

- workflow engine and run-state model
- API skeleton for orchestration
- in-memory storage adapter
- in-memory workflow queue and scheduler
- worker-pool-backed Docker executor
- worker runtime with multiple task executor types
- frontend dashboard prototype

Not everything is wired end-to-end yet, but the repository already reflects the intended architecture rather than a throwaway prototype.

## Local Install With Docker

The Docker-first local install path does not require Rust, Node.js, or a source checkout after installation. It uses prebuilt images by default and manages local config under `~/.runhelm`.

```bash
curl -fsSL https://raw.githubusercontent.com/markosski/runhelm/main/packaging/install.sh | sh
runhelm init
runhelm up
runhelm status
```

The generated config is written to `~/.runhelm/config.env`, and the generated Compose file is written to `~/.runhelm/docker-compose.yml`. Override image references there when using an internal registry:

```env
RUNHELM_ORCHESTRATOR_IMAGE=registry.example.com/runhelm-orchestrator:0
RUNHELM_WORKER_IMAGE=registry.example.com/runhelm-worker:0
RUNHELM_FRONTEND_IMAGE=registry.example.com/runhelm-frontend:0
```

Users who need to own their image artifacts can build them from a checkout or git ref:

```bash
packaging/build-images.sh --ref v0.3.1 --tag-prefix registry.example.com/runhelm --push
```

See `docs/local-install-distribution.md` for the release Compose template, wrapper commands, self-build path, and current persistence limits.

## Local Development

### Orchestrator

```bash
cd orchestrator
cargo run
```

The orchestrator starts a public Axum server on `0.0.0.0:3000` and a worker-only server on `127.0.0.1:3001`. Override them with `RUNHELM_PUBLIC_HTTP_ADDR` and `RUNHELM_WORKER_HTTP_ADDR`.

For a faster non-Docker development loop from the repository root:

```bash
scripts/run-orchestrator-dev.sh
scripts/run-orchestrator-dev.sh --skip-build
```

The script builds the debug binary with Cargo unless `--skip-build` is passed, then executes `orchestrator/target/debug/orchestrator` directly with local loopback HTTP defaults.

When running with Docker Compose, the orchestrator worker API is bound to `0.0.0.0:3001` inside the container so workers can reach it at `http://orchestrator:3001`. The Compose health check waits for both the public API and worker API before starting worker containers.

The repository root `docker-compose.yml` is the local development Compose file. It builds from `orchestrator/Dockerfile.dev` and `worker/Dockerfile.dev`. The production/release images use the main component Dockerfiles and are referenced by `packaging/docker-compose.release.yml`.

### Worker

```bash
cd worker
npm install
npm run build
npm start
```

The worker pulls tasks from the worker-only orchestrator API. Set `RUNHELM_ORCHESTRATOR_HTTP_URL` when the worker API is not reachable at the default local URL.

During container startup the worker retries registration until the orchestrator worker API is reachable. Short DNS or service-readiness races are logged as startup wait messages; a worker only exits on unrecoverable startup failures such as credential loading errors.

For a faster non-Docker single-worker loop from the repository root:

```bash
scripts/run-worker-dev.sh
scripts/run-worker-dev.sh --skip-build
```

The script builds the worker TypeScript unless `--skip-build` is passed, then runs `worker/dist/index.js` directly against `http://127.0.0.1:3001` by default. It sets `RUNHELM_WORKER_HOST_ID=local-dev-host` and a process-specific `WORKER_ID` unless those variables are already configured.

### Frontend

```bash
cd frontend
npm install
npm run dev
```

## Direction

RunHelm is aimed at teams building long-running, inspectable AI systems where workflows matter more than single prompts. The goal is a platform where agents are first-class workflow nodes, execution is isolated, contracts are typed, and operators can understand exactly what happened in every run.
