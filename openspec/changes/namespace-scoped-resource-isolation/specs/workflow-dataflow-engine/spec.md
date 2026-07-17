## ADDED Requirements

### Requirement: Namespace-Scoped Workflow Data
The orchestrator SHALL use namespace as part of definition and workflow-instance identity throughout registration, invocation, execution state, event persistence, task and verifier reads, listing, and storage projection updates.

#### Scenario: Definition identity is namespace-scoped
- **WHEN** two namespaces register workflow or function definitions with the same ID
- **THEN** each namespace reads, lists, invokes, overwrites, or deletes only its own definition

#### Scenario: Definition immutability is namespace-scoped
- **WHEN** one namespace has instantiated a workflow definition
- **AND** another namespace registers a definition with the same ID
- **THEN** the first namespace's instance does not prevent the second namespace from replacing its own definition

#### Scenario: Workflow state is namespace-scoped
- **WHEN** two namespaces contain workflow instances with the same ID
- **THEN** snapshot reads, task reads, event history, human input, retry, pause, resume, and commits affect only the selected namespace

#### Scenario: Definition invocation projection is namespace-scoped
- **WHEN** a workflow instance updates definition last-invoked metadata
- **THEN** storage updates only the workflow definition in the instance's namespace

#### Scenario: Namespace is absent from public definition body
- **WHEN** a caller registers or retrieves a workflow or function definition
- **THEN** the public definition payload does not gain a namespace routing field

