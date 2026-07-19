## ADDED Requirements

### Requirement: Namespace-Scoped Workflow Scheduling and Recovery
The orchestrator SHALL use one shared workflow queue to concurrently retain work from multiple namespaces and SHALL identify queued and active workflows by namespace and workflow instance ID. Startup recovery SHALL discover unfinished workflow information across all namespaces, while pause, resume, retry, requeue, and reconciliation operations after discovery SHALL use an explicit namespace.

#### Scenario: Identical workflow IDs are queued independently
- **WHEN** two namespaces enqueue the same workflow instance ID
- **THEN** the queue retains two independent composite entries
- **AND** completing or removing one entry does not affect the other

#### Scenario: Shared queue processes multiple namespaces
- **WHEN** one orchestrator process receives workflow work owned by different namespaces
- **THEN** the shared queue may schedule work from each namespace
- **AND** queue and in-flight state retain the owning namespace throughout processing

#### Scenario: Queue API is namespace-scoped
- **WHEN** a caller lists, removes, or purges queued work
- **THEN** the operation exposes or mutates only entries in the selected namespace

#### Scenario: Bulk control is namespace-scoped
- **WHEN** a caller bulk-pauses active workflows or bulk-resumes paused workflows
- **THEN** only workflows in the selected namespace are changed and queued or removed

#### Scenario: Cross-namespace startup recovery
- **WHEN** the orchestrator starts with unfinished workflow instances in multiple namespaces
- **THEN** startup task synchronization and active workflow requeue list unfinished workflow information without a namespace filter
- **AND** they recover and enqueue each instance using the namespace returned by storage

#### Scenario: Recovery is independent of default namespace
- **WHEN** the orchestrator starts with or without `RUNHELM_DEFAULT_NAMESPACE`
- **THEN** startup recovery considers unfinished workflow information from every namespace

#### Scenario: Recovery retains discovered namespace
- **WHEN** startup discovery returns workflow information from a namespace
- **THEN** all subsequent snapshot reads, state transitions, task synchronization, and queue actions use that namespace

#### Scenario: Reconciliation keeps namespace
- **WHEN** lost-host or retry processing identifies a workflow action
- **THEN** the resulting read, state transition, and queue action use the workflow's namespace
