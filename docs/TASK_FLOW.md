# RunHelm Task Submission and Execution Flow

This diagram shows the current task submission path through the Rust orchestrator, the in-memory workflow queue, the IPC-backed executor adapter, and the TypeScript worker runtime.

```mermaid
flowchart TD
    client["Client or UI"]
    api["Axum API"]
    orchestrator["Orchestrator"]
    storage["StoragePort<br/>workflow defs + instances"]
    queue["WorkflowQueuePort<br/>pending instance IDs"]
    scheduler["Workflow scheduler"]
    engine["WorkflowEngine"]
    executor["ExecutorPort<br/>DockerExecutor"]
    pool["WorkerPool"]
    socket["Unix socket IPC<br/>NDJSON messages"]
    worker["TypeScript worker"]
    factory["ExecutorFactory"]
    task_executor["Task executor<br/>Agent | ApiCall | Function"]
    external["LLM providers<br/>HTTP APIs<br/>function runtime"]

    client -->|"POST /workflow-def"| api
    api -->|"save WorkflowDef"| orchestrator
    orchestrator --> storage

    client -->|"POST /workflow-def/{def_id}"| api
    api -->|"create WorkflowInstance"| orchestrator
    orchestrator -->|"save Pending instance"| storage
    orchestrator -->|"enqueue instance ID"| queue

    scheduler -->|"dequeue instance ID"| queue
    scheduler -->|"run_workflow"| orchestrator
    orchestrator --> engine
    engine -->|"load WorkflowInstance + WorkflowDef"| storage
    engine -->|"initialize Pending TaskInstances"| storage

    engine -->|"find Pending tasks with satisfied data bindings"| engine
    engine -->|"mark task Running"| storage
    engine -->|"execute(workflow_def_id, task, inputs)"| executor
    executor -->|"dispatch_task"| pool
    pool -->|"choose idle worker<br/>mark Busy"| pool
    pool -->|"task_dispatch"| socket
    socket --> worker

    worker -->|"select by task.kind"| factory
    factory --> task_executor
    task_executor --> external
    external --> task_executor
    task_executor -->|"TaskExecutionResult"| worker
    worker -->|"task_result"| socket
    socket --> pool
    pool -->|"complete waiter<br/>mark worker Idle"| executor
    executor -->|"ExecutionResult"| engine

    engine -->|"Success: validate output schema"| engine
    engine -->|"Completed + output_data"| storage
    engine -->|"propagate output to downstream inputs"| storage
    engine -->|"InputNeeded or Failure"| storage
    engine -->|"all tasks Completed"| storage
```

## Task State Loop

```mermaid
stateDiagram-v2
    [*] --> Pending: TaskInstance created
    Pending --> Running: dependencies satisfied
    Running --> Completed: executor success + schema valid
    Running --> InputNeeded: worker requests user input
    Running --> Failed: executor error, task failure, or invalid output schema
    Completed --> [*]
    InputNeeded --> [*]
    Failed --> [*]
```

## Isolated Task Execution

`POST /workflow-def/{def_id}/tasks/{task_id}` bypasses the workflow queue and dataflow engine. It loads the registered workflow definition, finds one task, and sends that task directly to the configured executor with the request inputs.

```mermaid
sequenceDiagram
    participant Client
    participant API as Axum API
    participant Orch as Orchestrator
    participant Store as StoragePort
    participant Exec as DockerExecutor
    participant Pool as WorkerPool
    participant Worker as TypeScript worker

    Client->>API: POST /workflow-def/{def_id}/tasks/{task_id}
    API->>Orch: execute_workflow_task_isolated(def_id, task_id, inputs)
    Orch->>Store: get_workflow_def(def_id)
    Store-->>Orch: WorkflowDef
    Orch->>Exec: execute(def_id, task, inputs)
    Exec->>Pool: dispatch_task(def_id, task, inputs, timeout)
    Pool->>Worker: task_dispatch over Unix socket
    Worker->>Worker: execute via ExecutorFactory
    Worker-->>Pool: task_result
    Pool-->>Exec: ExecutionResult
    Exec-->>Orch: ExecutionResult
    Orch-->>API: success, input_needed, or failure
    API-->>Client: JSON response
```

