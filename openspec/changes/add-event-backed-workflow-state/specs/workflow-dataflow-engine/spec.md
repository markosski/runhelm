## ADDED Requirements

### Requirement: Workflow Instance Events
The orchestrator SHALL represent workflow instance state transitions as ordered domain events.

#### Scenario: Workflow transition emits event
- **WHEN** the workflow engine or workflow service changes workflow instance state
- **THEN** the change is representable as one or more workflow instance events

#### Scenario: Event order is meaningful
- **WHEN** multiple events are produced for one workflow instance transition
- **THEN** the system preserves their order for reducer application and persistence

### Requirement: Core Event Reduction
The orchestrator SHALL apply workflow instance events through core reducer logic outside storage adapters.

#### Scenario: Reducer applies task event
- **WHEN** a task attempt lifecycle event is applied to a workflow instance snapshot
- **THEN** core reducer logic updates the corresponding task attempt state

#### Scenario: Reducer applies verifier event
- **WHEN** a verifier state event is applied to a workflow instance snapshot
- **THEN** core reducer logic updates the verifier state and affected task attempt metadata

#### Scenario: Storage adapter does not interpret event
- **WHEN** a storage adapter receives workflow instance events to persist
- **THEN** it stores the events without applying workflow transition rules

### Requirement: Event-Backed Snapshot Persistence
The orchestrator SHALL persist workflow instance event batches and the resulting `WorkflowInstance` snapshot as one transition commit.

#### Scenario: Event batch updates snapshot
- **WHEN** core code commits an ordered batch of workflow instance events
- **THEN** the system applies the events to the current snapshot and persists the updated snapshot

#### Scenario: Event batch is persisted
- **WHEN** core code commits an ordered batch of workflow instance events
- **THEN** the system appends timestamped event records for that workflow instance

#### Scenario: Event batch commit updates summary
- **WHEN** core code commits an ordered batch of workflow instance events
- **THEN** storage receives the event records and reduced workflow snapshot in one commit operation
- **THEN** storage updates any lightweight summary projection from the reduced workflow snapshot

#### Scenario: Event record contains occurrence time
- **WHEN** core code commits workflow instance events
- **THEN** each persisted workflow event record includes a `created_time` value

#### Scenario: Empty event batch
- **WHEN** core code attempts to commit an empty event batch
- **THEN** the system rejects the operation without saving a snapshot

### Requirement: Snapshot-Backed Workflow Reads
The orchestrator SHALL continue serving full current workflow instance reads from snapshots and list queries from lightweight summary data produced with snapshots.

#### Scenario: Workflow instance read
- **WHEN** a caller requests a workflow instance by ID
- **THEN** the system returns the latest saved workflow instance snapshot

#### Scenario: Workflow list read
- **WHEN** a caller lists workflow instances
- **THEN** the system returns lightweight summary data produced by core from saved workflow instance snapshots
- **THEN** the list operation does not return full workflow instance state
- **THEN** the list operation does not load each full workflow instance snapshot to assemble the result

#### Scenario: Workflow list state filter
- **WHEN** a caller lists workflow instances with a workflow state filter
- **THEN** the system returns only summaries matching that filter

#### Scenario: Workflow list multi-state filter
- **WHEN** a caller lists workflow instances with multiple workflow states
- **THEN** the system returns only summaries whose workflow status is included in that set

#### Scenario: Active workflow discovery
- **WHEN** the orchestrator discovers active workflow instances
- **THEN** the system lists workflow summaries using a multi-state filter for pending and running workflow statuses

#### Scenario: Full workflow instance read is explicit
- **WHEN** a caller needs task inputs, task outputs, verifier history, or other full workflow instance state
- **THEN** the caller retrieves that state through the workflow instance get-by-ID operation

### Requirement: Workflow Instance Summary Fields
The orchestrator SHALL expose storage-level workflow instance list results as lightweight summaries.

#### Scenario: Summary contains identity and state
- **WHEN** a workflow instance appears in a storage-level list result
- **THEN** the summary includes the workflow instance ID, workflow definition ID, and current workflow state

#### Scenario: Summary contains lifecycle timestamps
- **WHEN** lifecycle timestamps are available for a workflow instance
- **THEN** the summary includes created time and completed time

#### Scenario: Summary contains task counts
- **WHEN** a workflow instance appears in a storage-level list result
- **THEN** the summary includes total task count and completed task count produced from the saved snapshot

### Requirement: Storage Adapter Boundary
Storage adapters SHALL be responsible for persistence mechanics and SHALL NOT own workflow event semantics.

#### Scenario: Memory storage commits events
- **WHEN** memory storage commits workflow instance event records with a reduced snapshot
- **THEN** it stores the event records without deciding how their payloads affect workflow, task, or verifier state

#### Scenario: Commit receives reduced state
- **WHEN** storage commits workflow instance event records and snapshot state
- **THEN** the snapshot has already been produced by core logic
- **THEN** any summary projection is derived from the committed snapshot, not from event payload semantics

### Requirement: Atomic Transition Batches
The orchestrator SHALL treat an ordered event batch from one workflow decision as a single transition.

#### Scenario: Multi-event verifier transition
- **WHEN** a verifier rejection creates multiple state changes such as recording feedback, marking attempts unsatisfied, and materializing the next attempts
- **THEN** those changes are committed as one ordered event batch

#### Scenario: Durable storage transaction expectation
- **WHEN** a durable storage adapter implements event-backed snapshots
- **THEN** event append, snapshot save, and summary projection update for one transition batch are performed atomically by the adapter's persistence mechanism
