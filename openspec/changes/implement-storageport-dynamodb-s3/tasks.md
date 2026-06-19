## 1. Storage Foundations

- [ ] 1.1 Extract a shared helper for computing whether a `WorkflowInstance` is active, matching the current `MemoryStorage` rules.
- [ ] 1.2 Add focused tests for active workflow detection across workflow status, task status, verifier state, and inactive instances.
- [ ] 1.3 Add serialization round-trip tests for `WorkflowDef`, `FunctionDef`, and `WorkflowInstance` documents used by storage adapters.

## 2. AWS Adapter Structure

- [ ] 2.1 Add orchestrator AWS SDK dependencies for shared AWS config, DynamoDB, and S3.
- [ ] 2.2 Add an `AwsStorage` adapter module implementing `StoragePort`.
- [ ] 2.3 Define adapter configuration for DynamoDB table name, S3 bucket, S3 key prefix, and optional endpoint overrides for local testing.
- [ ] 2.4 Define internal metadata and document-key helpers for workflow definitions, function definitions, and workflow instances.

## 3. S3 Document Store

- [ ] 3.1 Implement JSON document writes to deterministic S3 keys for workflow definitions, function definitions, and workflow instances.
- [ ] 3.2 Implement JSON document reads from S3 with typed deserialization and contextual errors.
- [ ] 3.3 Implement S3 document deletion for reusable function definitions.
- [ ] 3.4 Add unit tests for S3 key generation and document serialization behavior.

## 4. DynamoDB Metadata Store

- [ ] 4.1 Implement metadata writes for workflow definitions with resource type, id, updated timestamp, version, and S3 pointer.
- [ ] 4.2 Implement metadata writes for function definitions with resource type, id, updated timestamp, version, and S3 pointer.
- [ ] 4.3 Implement metadata writes for workflow instances with workflow definition id, workflow status, active flag, updated timestamp, version, and S3 pointer.
- [ ] 4.4 Implement direct metadata reads by resource id for all supported resource types.
- [ ] 4.5 Implement function definition metadata deletion with a boolean result that matches the `StoragePort` contract.
- [ ] 4.6 Add unit tests for metadata key construction, active flag persistence inputs, and missing-resource handling.

## 5. StoragePort Behavior

- [ ] 5.1 Implement `save_workflow_def` by writing the S3 document first and then updating DynamoDB metadata.
- [ ] 5.2 Implement `save_function_def` by writing the S3 document first and then updating DynamoDB metadata.
- [ ] 5.3 Implement `save_workflow_instance` by writing the S3 document first and then updating DynamoDB metadata with queryable state.
- [ ] 5.4 Implement `get_workflow_def`, `get_function_def`, and `get_workflow_instance` by reading metadata and then loading the S3 document.
- [ ] 5.5 Return `Ok(None)` when metadata is absent and return an error when metadata exists but the S3 document cannot be loaded or deserialized.
- [ ] 5.6 Implement `list_workflow_instances` using the workflow-instance metadata index and S3 document reads.
- [ ] 5.7 Implement `list_active_workflow_instances` using the active metadata index and S3 document reads.
- [ ] 5.8 Implement `delete_function_def` by deleting DynamoDB metadata and the referenced S3 document.

## 6. Runtime Configuration

- [ ] 6.1 Add a storage factory that returns memory storage by default.
- [ ] 6.2 Add `RUNHELM_STORAGE_BACKEND=memory|aws` selection in orchestrator startup.
- [ ] 6.3 Validate required AWS backend configuration and fail startup with clear missing-variable errors.
- [ ] 6.4 Wire the AWS storage adapter into `main.rs` without changing public API routing behavior.

## 7. Verification

- [ ] 7.1 Add unit tests for backend selection defaults and AWS configuration validation.
- [ ] 7.2 Add adapter tests using mocked or fake DynamoDB/S3 client boundaries for save/read/list/delete behavior.
- [ ] 7.3 Add an ignored or opt-in integration test path for DynamoDB/S3 compatible services such as LocalStack.
- [ ] 7.4 Run orchestrator tests and fix regressions.

## 8. Documentation

- [ ] 8.1 Update `docs/` with the DynamoDB table shape, required GSIs, S3 key layout, and latest-state write behavior.
- [ ] 8.2 Document required environment variables and AWS region resolution.
- [ ] 8.3 Document minimum IAM permissions for DynamoDB and S3 access.
- [ ] 8.4 Document local development behavior, including memory storage default and optional LocalStack-compatible testing.
