# RunHelm Architectural TODOs

## Scalability & Performance Bottlenecks

- [ ] **Implement long-running workers (Queue-based)**
  - *Issue:* Currently, `DockerExecutor` spins up a new container and Node.js environment for every single task, causing severe cold-start latency.
  - *Action:* Move away from Orchestrator-managed Docker lifecycle. Implement a pool of standing worker containers that pull tasks from a message queue.

- [ ] **Parallelize task execution within workflows**
  - *Issue:* `WorkflowEngine::run_workflow_instance` executes tasks sequentially via a standard `for` loop, even if tasks have no dependencies on each other.
  - *Action:* Use `futures::future::join_all` or `tokio::spawn` to run independent tasks concurrently during DAG evaluation.

- [ ] **Decouple Orchestrator from task execution (Message Broker)**
  - *Issue:* The orchestrator holds an active `.await` for the entire workflow lifespan and talks directly to the local Docker daemon, creating a single point of failure and preventing horizontal scaling.
  - *Action:* Introduce a message broker (e.g., Redis, RabbitMQ) to distribute tasks asynchronously. The Orchestrator should push tasks and react to completion events rather than blocking on execution.

- [ ] **Optimize database I/O for workflow state**
  - *Issue:* Workflow instances are saved to storage synchronously after every single task finishes, which will overwhelm the DB under heavy concurrent load.
  - *Action:* Implement event sourcing (appending state changes), state caching, or batched updates to reduce the frequency of full workflow blob writes.
