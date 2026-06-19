## Why

RunHelm's orchestrator currently uses in-memory storage, so registered definitions and workflow progress are lost when the process exits. A durable cloud `StoragePort` backend is needed to support restart recovery, active workflow requeueing, and persistent function/workflow registration in deployed environments.

## What Changes

- Add a cloud storage implementation for the orchestrator `StoragePort`.
- Use DynamoDB for queryable state metadata, including resource identity, workflow status, active workflow indexing, update timestamps, and storage pointers.
- Use S3 for document-shaped data such as registered function definitions, workflow definitions, workflow instance documents, task outputs/results, and other large JSON payloads.
- Add runtime configuration for selecting the cloud storage adapter and supplying AWS table, bucket, region, and key-prefix settings.
- Preserve the existing `StoragePort` behavior for direct reads, saves, function deletion, workflow instance listing, and active workflow instance listing.
- Update docs to describe the cloud storage architecture, required AWS resources, configuration, and local/deployment usage.

## Capabilities

### New Capabilities

- `cloud-storage-backend`: Durable orchestrator storage backed by DynamoDB metadata and S3 document objects while satisfying the existing `StoragePort` contract.

### Modified Capabilities

None.

## Impact

- Affected code: `orchestrator/src/adapters`, `orchestrator/src/main.rs`, `orchestrator/Cargo.toml`, storage-related tests, and configuration loading.
- Affected systems: AWS DynamoDB and S3 become optional runtime dependencies when the cloud storage adapter is selected.
- Affected docs: `docs/` must describe setup, schema, IAM permissions, configuration variables, and operational behavior for the DynamoDB/S3 backend.
- API behavior should remain compatible because the adapter is behind `StoragePort`.
