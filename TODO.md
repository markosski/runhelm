# RunHelm Architectural TODOs

## MVP Solidification

- [ ] **Add durable local storage**
  - *Issue:* The orchestrator currently wires `MemoryStorage`, so workflow definitions, function definitions, workflow instances, task state, and task results are lost when the process restarts.
  - *Action:* Add a durable `StoragePort` implementation, preferably SQLite first, with data stored under `~/.runhelm/` for local MVP installs. Keep the existing in-memory adapter for tests and lightweight development.

- [ ] **Make workflow trigger payloads first-class run inputs**
  - *Issue:* `POST /workflow-def/{def_id}` accepts a JSON payload but currently ignores it, so workflow runs cannot be meaningfully parameterized by caller-provided input.
  - *Action:* Persist workflow run inputs on `WorkflowInstance`, define how initial inputs bind into task inputs, and expose the run input metadata through status/result APIs where appropriate.

- [ ] **Improve run and task observability**
  - *Issue:* Workflow status currently exposes task state and whether output exists, but it does not provide enough information to debug real user workflows.
  - *Action:* Track and expose run/task timestamps, failure reasons, timeout reasons, assigned worker IDs when available, and task result/error details in a stable API shape.

- [ ] **Wire the frontend to real backend APIs**
  - *Issue:* The frontend is currently a static operator-console prototype with mock workflow and run data.
  - *Action:* Connect the UI to the orchestrator APIs for workflow lists, run lists, queue state, task status, and task result inspection. Keep the first integrated UI narrow and focused on observing existing workflows rather than designing new ones.

- [ ] **Implement local install and distribution path**
  - *Issue:* Local installation is documented, but users still need repository knowledge and local build tooling to run RunHelm.
  - *Action:* Implement the Docker-first local distribution plan: release Compose file, versioned service images, `runhelm init`, `runhelm up`, `runhelm down`, `runhelm status`, `runhelm logs`, and `runhelm doctor`.

## Scalability & Performance Bottlenecks

- [x] **Implement long-running workers (Queue-based)**
  - *Issue:* Currently, `DockerExecutor` spins up a new container and Node.js environment for every single task, causing severe cold-start latency.
  - *Action:* Move away from Orchestrator-managed Docker lifecycle. Implement a pool of standing worker containers that pull tasks from a message queue.

- [x] **Parallelize task execution within workflows**
  - *Issue:* `WorkflowEngine::run_workflow_instance` executes tasks sequentially via a standard `for` loop, even if tasks have no dependencies on each other.
  - *Action:* Use `futures::future::join_all` or `tokio::spawn` to run independent tasks concurrently during DAG evaluation.

- [x] **Decouple Orchestrator from task execution (Message Broker)**
  - *Issue:* The orchestrator holds an active `.await` for the entire workflow lifespan and talks directly to the local Docker daemon, creating a single point of failure and preventing horizontal scaling.
  - *Action:* Introduce a message broker (e.g., Redis, RabbitMQ) to distribute tasks asynchronously. The Orchestrator should push tasks and react to completion events rather than blocking on execution.

- [ ] **Optimize database I/O for workflow state**
  - *Issue:* Workflow instances are saved to storage synchronously after every single task finishes, which will overwhelm the DB under heavy concurrent load.
  - *Action:* Implement event sourcing (appending state changes), state caching, or batched updates to reduce the frequency of full workflow blob writes.

- [ ] **Cache function task dependencies on the host**
  - *Issue:* Function-type tasks with dependencies may reinstall packages repeatedly, increasing execution latency and wasting network and disk work.
  - *Action:* Ensure dependencies are cached on the host machine at `/tmp/runhelm/npm/<workflow_def_name>/<task_id>/` when executing function-type tasks with dependencies.

## Workflow Orchestration Capabilities

- [ ] **Define workflow definition versioning strategy**
  - *Issue:* Workflow definitions do not yet have a clear versioning model, which makes it unclear how old and new versions should be maintained, referenced, and executed over time.
  - *Action:* Decide whether workflow versions should be explicit in workflow names or stored at the definition level, and define how version history, compatibility, migration, and execution of older versions should work.

## Agent Intelligence & Memory

- [ ] **Investigate self-learning for agent tasks**
  - *Issue:* Agent tasks currently execute without a scoped memory of prior executions, so they cannot adapt based on past outcomes for the same workflow, task, or user-defined scope.
  - *Action:* Explore scoped execution memory for agent tasks, including what should be persisted, how memories are retrieved and summarized, how learning is bounded to the correct scope, and what controls are needed for observability, privacy, and reset behavior.

## Installation & Distribution

- [ ] **Make RunHelm easy to install locally**
  - *Issue:* Users currently need to understand the repository layout and manually run the orchestrator, worker, frontend, and supporting services.
  - *Action:* Add the packaging, installer scripts, default configuration, dependency checks, documentation, and startup commands needed for users to install and run RunHelm on their own computer with minimal manual setup.

## Dismissed Tasks

Tasks that were considered but later deemed unnecessary.

- [x] **Execute workflows on a schedule**
  - *Issue:* The orchestrator can execute workflows on demand, but it cannot currently trigger workflows automatically on a recurring schedule.
  - *Dismissal reason:* RunHelm exposes an API, so recurring scheduling should be handled by external applications such as `crontab`.
