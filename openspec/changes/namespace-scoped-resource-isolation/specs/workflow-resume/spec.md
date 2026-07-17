## ADDED Requirements

### Requirement: Namespace-Scoped Workflow Scheduling and Recovery
The orchestrator SHALL retain namespace with queued workflow work and SHALL scope pause, resume, retry, recovery, requeue, and reconciliation operations to an explicit namespace.

#### Scenario: Identical workflow IDs are queued independently
- **WHEN** two namespaces enqueue the same workflow instance ID
- **THEN** the queue retains two independent composite entries
- **AND** completing or removing one entry does not affect the other

#### Scenario: Queue API is namespace-scoped
- **WHEN** a caller lists, removes, or purges queued work
- **THEN** the operation exposes or mutates only entries in the selected namespace

#### Scenario: Bulk control is namespace-scoped
- **WHEN** a caller bulk-pauses active workflows or bulk-resumes paused workflows
- **THEN** only workflows in the selected namespace are changed and queued or removed

#### Scenario: Default namespace startup recovery
- **WHEN** the orchestrator starts with a configured default namespace
- **THEN** startup task synchronization and active workflow requeue inspect only that namespace

#### Scenario: Startup without namespace discovery
- **WHEN** the orchestrator starts without a configured default namespace
- **THEN** it skips workflow recovery and requeue with an observable diagnostic
- **AND** it does not perform a global resource scan

#### Scenario: Reconciliation keeps namespace
- **WHEN** lost-host or retry processing identifies a workflow action
- **THEN** the resulting read, state transition, and queue action use the workflow's namespace

