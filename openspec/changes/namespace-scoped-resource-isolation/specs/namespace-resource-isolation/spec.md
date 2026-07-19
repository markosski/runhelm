## ADDED Requirements

### Requirement: Validated Namespace Identity
RunHelm SHALL represent namespace identity as a string containing a UUID in canonical hyphenated form.

#### Scenario: Valid namespace is accepted
- **WHEN** a canonical UUID string such as `550e8400-e29b-41d4-a716-446655440000` is supplied by trusted configuration or a namespace resolver
- **THEN** RunHelm constructs the namespace value
- **AND** internal contracts serialize the namespace as that string

#### Scenario: Invalid namespace is rejected
- **WHEN** a non-empty namespace value is not a canonical hyphenated UUID string
- **THEN** RunHelm rejects the namespace text before performing a resource action

### Requirement: Public Request Namespace Selection
RunHelm SHALL resolve a namespace context before each public resource handler performs an action and SHALL keep health checks namespace-independent.

#### Scenario: Configured default takes precedence
- **WHEN** `RUNHELM_DEFAULT_NAMESPACE` contains a non-empty valid namespace
- **AND** a public resource request includes any authorization header or no authorization header
- **THEN** RunHelm selects the configured default namespace
- **AND** RunHelm does not inspect or resolve the supplied API key

#### Scenario: Empty default is absent
- **WHEN** `RUNHELM_DEFAULT_NAMESPACE` is absent, empty, or whitespace-only
- **THEN** RunHelm does not select a default namespace

#### Scenario: Missing bearer credential
- **WHEN** no default namespace is configured
- **AND** a public resource request omits `Authorization: Bearer <api-key>`
- **THEN** RunHelm returns `401 Unauthorized`
- **AND** no resource action occurs

#### Scenario: Malformed bearer credential
- **WHEN** no default namespace is configured
- **AND** a public resource request supplies a malformed or empty bearer credential
- **THEN** RunHelm returns `401 Unauthorized`
- **AND** no resource action occurs

#### Scenario: Presented API key reaches deferred resolver
- **WHEN** no default namespace is configured
- **AND** a public resource request supplies a well-formed `Authorization: Bearer <api-key>` credential
- **THEN** RunHelm passes the API key to the namespace resolver boundary
- **AND** the resolver deliberately panics as not implemented in this story
- **AND** RunHelm does not select a fallback namespace

#### Scenario: Health check without namespace context
- **WHEN** a caller invokes a health-check endpoint without default namespace configuration or authorization
- **THEN** RunHelm returns the health response without invoking namespace resolution

### Requirement: Namespace Ownership Boundary
RunHelm SHALL evaluate every definition, workflow instance, task, verifier state, event, queue entry, and related mutation using the selected namespace as part of resource identity.

#### Scenario: Identical IDs in separate namespaces
- **WHEN** two namespaces create resources of the same kind with the same identifier
- **THEN** each namespace retains an independent resource
- **AND** operations in either namespace do not read or modify the other resource

#### Scenario: Cross-namespace point access is absent
- **WHEN** a namespace requests or mutates an identifier that exists only in another namespace
- **THEN** RunHelm behaves as if the identifier does not exist in the selected namespace
- **AND** the response does not reveal the other namespace's resource

#### Scenario: Namespace-scoped collection
- **WHEN** a namespace lists definitions, workflow instances, events, tasks, or queue entries
- **THEN** every returned item belongs to the selected namespace

#### Scenario: Resource payload cannot select namespace
- **WHEN** a caller submits a definition or action payload
- **THEN** RunHelm derives namespace only from request context
- **AND** public resource fields cannot override the selected namespace

### Requirement: Namespace-Scoped Storage Adapters
Every active storage adapter SHALL encode namespace ownership in authoritative keys, relationships, queries, projections, and immutable payload locations rather than applying an in-memory result filter after a cross-namespace resource read. Storage-facing workflow information SHALL carry its owning namespace. `list_workflow_info` SHALL accept an optional namespace, with a missing namespace reserved for startup recovery; every other storage operation SHALL require an explicit namespace.

#### Scenario: Namespace-scoped workflow information listing
- **WHEN** a normal service calls `list_workflow_info` with a namespace
- **THEN** storage returns only workflow information owned by that namespace
- **AND** the public workflow-list response omits the storage-facing namespace field

#### Scenario: Recovery workflow information listing
- **WHEN** startup recovery calls `list_workflow_info` without a namespace
- **THEN** storage returns matching workflow information across all namespaces
- **AND** every returned item identifies its owning namespace
- **AND** pagination distinguishes otherwise identical workflow IDs across namespaces

#### Scenario: Memory storage isolation
- **WHEN** memory storage contains identical resource IDs in two namespaces
- **THEN** point reads, lists, saves, deletes, event reads, and workflow commits use independent composite identities

#### Scenario: SQL storage isolation
- **WHEN** SQL storage persists definitions, workflow instances, tasks, verifier state, and events
- **THEN** their primary keys and relationships include namespace
- **AND** joins, filters, pagination, uniqueness checks, and indexes constrain namespace

#### Scenario: AWS storage isolation
- **WHEN** AWS storage persists or queries a namespaced resource
- **THEN** DynamoDB keys and list projections include the namespace component
- **AND** S3 object paths include the namespace component
- **AND** the adapter does not scan or fetch records from other namespaces and filter them client-side

#### Scenario: Version conflict is namespace-local
- **WHEN** two workflow instances share an ID in different namespaces and one instance commits a transition
- **THEN** optimistic version validation is evaluated only against the instance in that namespace

### Requirement: Destructive Namespace Schema Boundary
RunHelm SHALL define namespace ownership in a reset SQL initial schema and SHALL NOT migrate a pre-namespace database as part of this change.

#### Scenario: Fresh SQL database initialization
- **WHEN** SQL storage starts with a fresh database
- **THEN** it creates the namespace-aware initial schema including all current definition metadata

#### Scenario: Existing pre-namespace database
- **WHEN** an operator upgrades to this change with an existing pre-namespace SQL database
- **THEN** the documented deployment procedure requires recreating the database rather than applying an ownership migration
