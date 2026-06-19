## ADDED Requirements

### Requirement: Configurable Storage Backend Selection
The orchestrator SHALL select its `StoragePort` implementation from runtime configuration while preserving memory storage as the default backend.

#### Scenario: Default storage backend
- **WHEN** the orchestrator starts without `RUNHELM_STORAGE_BACKEND`
- **THEN** the system uses the in-memory storage backend

#### Scenario: AWS storage backend selected
- **WHEN** the orchestrator starts with `RUNHELM_STORAGE_BACKEND` set to `aws`
- **THEN** the system initializes the DynamoDB and S3 backed storage adapter

#### Scenario: Missing AWS storage configuration
- **WHEN** the orchestrator starts with `RUNHELM_STORAGE_BACKEND` set to `aws` and required table or bucket configuration is missing
- **THEN** the system fails startup with an error that identifies the missing configuration

### Requirement: DynamoDB Metadata and S3 Document Persistence
The AWS storage backend SHALL store queryable resource metadata in DynamoDB and serialized resource documents in S3 for workflow definitions, function definitions, and workflow instances.

#### Scenario: Workflow definition is saved
- **WHEN** a workflow definition is saved through `StoragePort`
- **THEN** the system writes the workflow definition document to S3
- **THEN** the system writes DynamoDB metadata that identifies the workflow definition and points to the S3 document

#### Scenario: Function definition is saved
- **WHEN** a function definition is saved through `StoragePort`
- **THEN** the system writes the function definition document to S3
- **THEN** the system writes DynamoDB metadata that identifies the function definition and points to the S3 document

#### Scenario: Workflow instance is saved
- **WHEN** a workflow instance is saved through `StoragePort`
- **THEN** the system writes the workflow instance document to S3
- **THEN** the system writes DynamoDB metadata that identifies the workflow instance, records queryable workflow state, and points to the S3 document

### Requirement: StoragePort Read Behavior
The AWS storage backend SHALL satisfy the existing `StoragePort` direct-read contract by loading metadata from DynamoDB and the corresponding document from S3.

#### Scenario: Existing workflow definition is read
- **WHEN** a stored workflow definition is requested by id
- **THEN** the system returns the deserialized workflow definition from S3

#### Scenario: Existing function definition is read
- **WHEN** a stored function definition is requested by id
- **THEN** the system returns the deserialized function definition from S3

#### Scenario: Existing workflow instance is read
- **WHEN** a stored workflow instance is requested by id
- **THEN** the system returns the deserialized workflow instance from S3

#### Scenario: Missing resource is read
- **WHEN** a resource id has no DynamoDB metadata item
- **THEN** the system returns no resource

#### Scenario: Metadata points to missing document
- **WHEN** a DynamoDB metadata item exists but its referenced S3 document cannot be loaded
- **THEN** the system returns a storage error instead of returning no resource

### Requirement: Workflow Instance Listing
The AWS storage backend SHALL list workflow instances through DynamoDB metadata queries and return deserialized workflow instance documents.

#### Scenario: Workflow instances are listed
- **WHEN** workflow instances are listed through `StoragePort`
- **THEN** the system queries workflow instance metadata from DynamoDB
- **THEN** the system returns the corresponding workflow instance documents from S3

#### Scenario: No workflow instances exist
- **WHEN** workflow instances are listed and no workflow instance metadata exists
- **THEN** the system returns an empty list

### Requirement: Active Workflow Instance Indexing
The AWS storage backend SHALL maintain an indexed active flag for workflow instances and use it to list active workflow instances without scanning all stored workflow documents.

#### Scenario: Active workflow status is saved
- **WHEN** a workflow instance with workflow status `Pending` or `Running` is saved
- **THEN** the system stores DynamoDB metadata that marks the workflow instance active

#### Scenario: Active task status is saved
- **WHEN** a workflow instance with any task status `Pending` or `Running` is saved
- **THEN** the system stores DynamoDB metadata that marks the workflow instance active

#### Scenario: Active verifier state is saved
- **WHEN** a workflow instance with any verifier state `Running` is saved
- **THEN** the system stores DynamoDB metadata that marks the workflow instance active

#### Scenario: Inactive workflow instance is saved
- **WHEN** a workflow instance has no active workflow status, no active task status, and no running verifier state
- **THEN** the system stores DynamoDB metadata that marks the workflow instance inactive

#### Scenario: Active workflow instances are listed
- **WHEN** active workflow instances are listed through `StoragePort`
- **THEN** the system queries the active workflow metadata index
- **THEN** the system returns only workflow instances marked active

### Requirement: Function Definition Deletion
The AWS storage backend SHALL delete reusable function definitions from both queryable metadata and document storage.

#### Scenario: Existing function definition is deleted
- **WHEN** an existing function definition is deleted by id
- **THEN** the system removes the function definition metadata from DynamoDB
- **THEN** the system removes the referenced function definition document from S3
- **THEN** the delete operation returns `true`

#### Scenario: Missing function definition is deleted
- **WHEN** a function definition id has no DynamoDB metadata item
- **THEN** the delete operation returns `false`

### Requirement: Latest-State Write Semantics
The AWS storage backend SHALL preserve the current `StoragePort` latest-state semantics while maintaining metadata useful for future concurrency control.

#### Scenario: Resource is overwritten
- **WHEN** a workflow definition, function definition, or workflow instance is saved with an id that already exists
- **THEN** the system updates the S3 document and DynamoDB metadata for that id as the latest state

#### Scenario: Metadata version is updated
- **WHEN** a stored resource is saved
- **THEN** the system records or advances a DynamoDB metadata version for that resource

### Requirement: Cloud Storage Documentation
The system documentation SHALL describe how to configure and operate the DynamoDB and S3 storage backend.

#### Scenario: Operator configures AWS storage
- **WHEN** an operator reads the storage backend documentation
- **THEN** the documentation identifies required environment variables, DynamoDB table/index expectations, S3 bucket/key layout, and IAM permissions

#### Scenario: Operator keeps local storage
- **WHEN** an operator reads the storage backend documentation for local development
- **THEN** the documentation explains that memory storage remains the default backend when cloud storage is not selected
