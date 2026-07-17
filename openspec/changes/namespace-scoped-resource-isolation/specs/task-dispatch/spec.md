## ADDED Requirements

### Requirement: Namespace-Aware Task Dispatch
The task dispatch port SHALL receive the namespace that owns the workflow execution and SHALL retain it in every worker-facing dispatch payload and namespace-derived execution identity.

#### Scenario: Engine dispatches namespaced task
- **WHEN** the engine dispatches a ready task for a workflow instance
- **THEN** it supplies the workflow namespace together with workflow instance ID, task, inputs, metadata, and constraints

#### Scenario: Isolated definition task is namespaced
- **WHEN** a caller executes one workflow-definition task in isolation
- **THEN** the dispatch identity and worker payload retain the selected namespace

#### Scenario: Workspace and session identity are namespaced
- **WHEN** two namespaces execute the same workflow and logical task IDs
- **THEN** worker workspace and reusable Agent session identities do not collide between the namespaces

