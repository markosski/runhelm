## ADDED Requirements

### Requirement: Account inbound email address
The system SHALL support one inbound email address per account for receiving workflow requests.

#### Scenario: Email maps to account
- **WHEN** an inbound email is received for an account email address
- **THEN** the system identifies the target account before attempting workflow resolution

#### Scenario: Unknown account address
- **WHEN** an inbound email is received for an address that is not assigned to an account
- **THEN** the system rejects processing without creating a workflow run

### Requirement: Inbound email normalization and persistence
The system SHALL normalize inbound provider email events and persist the inbound message before workflow resolution or execution.

#### Scenario: Message is persisted before processing
- **WHEN** a provider webhook delivers an inbound email
- **THEN** the system stores normalized message metadata, sender, recipients, subject, body, provider identifiers, and processing status before resolving a workflow

#### Scenario: Attachments are stored as references
- **WHEN** an inbound email includes attachments
- **THEN** the system stores attachment metadata and content references that can be passed to workflow execution

#### Scenario: Duplicate provider delivery
- **WHEN** a provider retries delivery for an already processed inbound email
- **THEN** the system detects the duplicate and does not create a second workflow run for the same message

### Requirement: Sender authorization
The system SHALL authorize the sender against account and workflow email trigger policy before executing a workflow.

#### Scenario: Authorized sender
- **WHEN** an inbound email sender is allowed by account or workflow email policy
- **THEN** the system may continue workflow resolution for that sender

#### Scenario: Unauthorized sender
- **WHEN** an inbound email sender is not allowed by account or workflow email policy
- **THEN** the system rejects the request and does not execute a workflow

### Requirement: Email-enabled workflow candidates
The system SHALL only consider workflows that are explicitly enabled for email triggering.

#### Scenario: Workflow is email enabled
- **WHEN** a workflow is enabled for email triggering
- **THEN** the workflow is eligible for deterministic and LLM-based resolution

#### Scenario: Workflow is not email enabled
- **WHEN** a workflow is not enabled for email triggering
- **THEN** the workflow is excluded from email workflow resolution

### Requirement: Deterministic workflow resolution
The system SHALL attempt deterministic workflow resolution before using LLM inference.

#### Scenario: Exact workflow handle match
- **WHEN** an inbound email contains an exact email-enabled workflow handle
- **THEN** the system selects the matching workflow without LLM inference

#### Scenario: Exact workflow name match
- **WHEN** an inbound email contains an exact email-enabled workflow name
- **THEN** the system selects the matching workflow without LLM inference

#### Scenario: Workflow alias match
- **WHEN** an inbound email contains a configured alias for an email-enabled workflow
- **THEN** the system selects the matching workflow without LLM inference

### Requirement: LLM workflow inference
The system SHALL use LLM inference to propose a workflow only when deterministic resolution does not select a workflow.

#### Scenario: Inference uses eligible workflow metadata
- **WHEN** deterministic resolution does not select a workflow
- **THEN** the system invokes LLM inference using only metadata for workflows that are eligible for the sender and enabled for email

#### Scenario: Inference produces a candidate
- **WHEN** LLM inference identifies a likely workflow
- **THEN** the system stores the candidate workflow and asks the sender to confirm before execution

#### Scenario: Inference is ambiguous
- **WHEN** LLM inference identifies multiple plausible workflows or insufficient confidence
- **THEN** the system replies with a clarification or selection prompt instead of executing a workflow

### Requirement: Confirmation policy
The system SHALL require confirmation for LLM-inferred workflow selections and for workflows whose email policy requires confirmation.

#### Scenario: LLM-inferred selection requires confirmation
- **WHEN** a workflow is selected through LLM inference
- **THEN** the system sends a confirmation reply and waits for sender confirmation before executing the workflow

#### Scenario: Workflow policy requires confirmation
- **WHEN** a selected workflow has an email policy that requires confirmation
- **THEN** the system sends a confirmation reply and waits for sender confirmation before executing the workflow

#### Scenario: Confirmation rejected
- **WHEN** the sender rejects a pending confirmation
- **THEN** the system marks the email interaction as rejected and does not execute the workflow

### Requirement: Email interaction state
The system SHALL persist pending email interaction state for confirmations, missing inputs, workflow runs, and follow-up replies.

#### Scenario: Confirmation reply resumes pending interaction
- **WHEN** the sender replies to a confirmation prompt with approval
- **THEN** the system resumes the pending interaction and proceeds without re-running workflow inference

#### Scenario: Missing input reply resumes pending interaction
- **WHEN** the sender replies to a missing-input prompt
- **THEN** the system applies the reply to the pending interaction and revalidates the workflow inputs

#### Scenario: Pending interaction expires
- **WHEN** a pending email interaction has expired
- **THEN** the system refuses to resume it and requires a new workflow request

### Requirement: Input extraction and validation
The system SHALL extract workflow inputs from inbound email content and validate them against the selected workflow input requirements before execution.

#### Scenario: Required inputs are present
- **WHEN** all required inputs are extracted from the email subject, body, sender context, or attachments
- **THEN** the system creates a workflow run request with validated inputs

#### Scenario: Required inputs are missing
- **WHEN** required workflow inputs cannot be extracted
- **THEN** the system replies asking the sender for the missing inputs instead of executing the workflow

#### Scenario: Extracted inputs are invalid
- **WHEN** extracted inputs do not satisfy the selected workflow input requirements
- **THEN** the system replies with an input correction prompt instead of executing the workflow

### Requirement: Email workflow execution
The system SHALL execute confirmed and validated email workflow requests through the workflow execution entrypoint using email trigger metadata.

#### Scenario: Workflow run starts
- **WHEN** sender authorization, workflow selection, confirmation policy, and input validation are satisfied
- **THEN** the system starts a workflow run with trigger source `email`

#### Scenario: Trigger metadata is available to execution
- **WHEN** a workflow run is started from email
- **THEN** the workflow run includes account, sender, inbound message, selected workflow, validated inputs, attachment references, and reply thread metadata

### Requirement: Email replies
The system SHALL reply to the email thread with confirmation prompts, missing-input prompts, run-start notifications, final results, and errors.

#### Scenario: Long-running workflow starts
- **WHEN** an email-triggered workflow run is accepted but not complete
- **THEN** the system replies that the workflow has started and later replies with completion or failure

#### Scenario: Workflow completes successfully
- **WHEN** an email-triggered workflow run completes successfully
- **THEN** the system replies to the email thread with the workflow result or result summary

#### Scenario: Workflow fails
- **WHEN** an email-triggered workflow run fails
- **THEN** the system replies to the email thread with an error message suitable for the sender

### Requirement: Automated email loop protection
The system SHALL avoid triggering workflows or reply loops from automated email responses.

#### Scenario: Auto-reply detected
- **WHEN** an inbound email contains provider-supported or standard auto-reply indicators
- **THEN** the system suppresses workflow execution for that message

#### Scenario: Reply loop risk detected
- **WHEN** sending a reply would target an automated sender or otherwise risk a reply loop
- **THEN** the system suppresses or limits the reply according to email safety policy
