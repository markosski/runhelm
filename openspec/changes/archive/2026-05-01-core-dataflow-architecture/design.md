## Context

The RunHelm Orchestrator is currently structured using Ports and Adapters but lacks a concrete workflow execution engine. To support complex, non-deterministic agentic workflows effectively, we need to transition to a dataflow-centric architecture where tasks run dynamically based on data availability, rather than a rigid linear pipeline.

## Goals / Non-Goals

**Goals:**
- Implement the core struct definitions for `TaskDef`, `WorkflowDef`, `TaskInstance`, and `WorkflowInstance`.
- Support multiple inputs for a single task (Fan-In) via `input_schemas` and `DataBinding` definitions.
- Implement strict JSON schema validation for all task outputs before data is propagated downstream.

**Non-Goals:**
- We are not implementing the actual agents or integrations in this phase.
- We are not implementing a persistent distributed database storage layer (we will use the existing `MemoryStorage` for now).
- Visual editor or UI for building these DAGs.

## Decisions

**1. Dataflow Over Explicit Graph Steps**
- *Rationale*: Agentic workflows often have unpredictable paths or parallelizable tasks. By relying on `DataBinding`s (where output from A feeds input of B), the engine can inherently infer the Directed Acyclic Graph (DAG) and parallelize execution natively without the user having to manually define "Step 1, Step 2, Step 3".

**2. Strict Schema Validation at Runtime**
- *Rationale*: LLM outputs are non-deterministic. The Orchestrator must serve as a strict boundary. If an agent outputs malformed JSON that fails the `output_schema` validation, the Orchestrator marks the task as `Failed` and halts propagation, protecting downstream tasks from silent data corruption.

**3. Separation of Definition and Instance**
- *Rationale*: Allows workflows to be highly reusable templates. We avoid mutating definitions during execution, making the state of an instance completely predictable and independently verifiable.

## Risks / Trade-offs

- **Risk:** Cyclic dependencies in `DataBinding`s.
  - **Mitigation:** The Orchestrator engine must run a cycle-detection check (e.g., topological sort) during the `WorkflowInstance` initialization phase before transitioning any tasks to `Running`.
- **Risk:** Schema Validation overhead. Validating large JSON payloads dynamically might introduce CPU overhead.
  - **Mitigation:** Use an optimized Rust JSON schema validator crate like `jsonschema`.
