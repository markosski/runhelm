# RunHelm Architectural TODOs

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

- [ ] **Execute workflows on a schedule**
  - *Issue:* The orchestrator can execute workflows on demand, but it cannot currently trigger workflows automatically on a recurring schedule.
  - *Action:* Add scheduler support to register cron-like or interval-based workflow triggers, persist schedule definitions, and enqueue workflow runs at the configured times.

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
