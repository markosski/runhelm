## 1. Core Models Setup

- [x] 1.1 Add `jsonschema` and `serde_json` dependencies to `Cargo.toml` if not present
- [x] 1.2 Update `TaskKindDef` and `TaskDef` structs to support `input_schemas`, `output_schema`, and `expected_side_effects`
- [x] 1.3 Create `DataBinding` and `WorkflowDef` structs to map data flow
- [x] 1.4 Create `TaskInstance`, `SideEffectInstance`, and `WorkflowInstance` structs for live execution state

## 2. Storage Port Refactoring

- [x] 2.1 Update `StoragePort` trait to handle `WorkflowDef`s separately from `WorkflowInstance`s
- [x] 2.2 Update `MemoryStorage` adapter to implement new `StoragePort` methods

## 3. Workflow Engine Foundation

- [x] 3.1 Create basic engine struct and entry point `run_workflow_instance(instance_id: String)`
- [x] 3.2 Implement cycle detection and DAG construction for `WorkflowDef` data bindings

## 4. Execution Loop and Validation

- [x] 4.1 Implement strict JSON schema validation for task outputs using `jsonschema`
- [x] 4.2 Implement dataflow loop to transition tasks from `Pending` to `Running` when inputs are satisfied
- [x] 4.3 Implement data propagation from successful task outputs to downstream `TaskInstance` input payloads
- [x] 4.4 Implement `Failed` state handling when schema validation fails or a task errors
