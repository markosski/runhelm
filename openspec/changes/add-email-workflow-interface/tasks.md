## 1. Domain Model And Interfaces

- [ ] 1.1 Define email trigger domain types for inbound messages, attachments, account email settings, workflow email metadata, sender policy, and interaction state
- [ ] 1.2 Add repository interfaces for storing inbound messages, attachment records, account address mappings, sender policies, and email interactions
- [ ] 1.3 Add provider-facing interfaces for webhook verification, inbound message normalization, attachment storage, and threaded reply sending
- [ ] 1.4 Add workflow trigger metadata types for email-sourced workflow run requests

## 2. Inbound Email Intake

- [ ] 2.1 Implement inbound email webhook handling behind the selected provider adapter
- [ ] 2.2 Verify provider webhook authenticity before accepting inbound email payloads
- [ ] 2.3 Normalize provider payloads into RunHelm inbound email messages
- [ ] 2.4 Persist normalized inbound messages before workflow resolution starts
- [ ] 2.5 Store inbound attachment metadata and content references
- [ ] 2.6 Add deduplication using provider identifiers and email message headers
- [ ] 2.7 Detect automated email indicators and suppress workflow triggering for auto-replies

## 3. Account And Authorization Policy

- [ ] 3.1 Implement lookup from inbound recipient address to account
- [ ] 3.2 Reject inbound email for unknown account addresses without creating workflow runs
- [ ] 3.3 Implement account-level sender allowlist or domain allowlist checks
- [ ] 3.4 Implement workflow-level email enablement and confirmation policy checks
- [ ] 3.5 Ensure unauthorized senders cannot trigger workflow resolution or execution

## 4. Workflow Resolution

- [ ] 4.1 Build candidate workflow loading limited to sender-authorized email-enabled workflows
- [ ] 4.2 Implement exact workflow handle matching against inbound email subject and body
- [ ] 4.3 Implement exact workflow name matching against inbound email subject and body
- [ ] 4.4 Implement configured workflow alias matching
- [ ] 4.5 Implement LLM workflow inference using only eligible workflow metadata when deterministic resolution fails
- [ ] 4.6 Persist workflow resolution results, confidence, alternatives, and rationale for audit/debugging
- [ ] 4.7 Reply with a clarification prompt when workflow resolution is ambiguous

## 5. Confirmation And Interaction State

- [ ] 5.1 Create pending email interactions for LLM-inferred workflow selections
- [ ] 5.2 Create pending email interactions for workflows whose email policy requires confirmation
- [ ] 5.3 Resume pending interactions from confirmation replies without re-running workflow inference
- [ ] 5.4 Handle sender rejection of pending confirmations
- [ ] 5.5 Expire stale pending interactions and require a new request after expiration

## 6. Input Extraction And Validation

- [ ] 6.1 Extract candidate workflow inputs from email subject, body, sender context, and attachment references
- [ ] 6.2 Validate extracted inputs against the selected workflow input requirements
- [ ] 6.3 Reply with missing-input prompts when required inputs are absent
- [ ] 6.4 Resume pending interactions from missing-input replies and revalidate inputs
- [ ] 6.5 Reply with correction prompts when extracted inputs are invalid

## 7. Workflow Execution And Replies

- [ ] 7.1 Start email-triggered workflow runs through the existing workflow execution entrypoint
- [ ] 7.2 Include email trigger metadata in workflow run requests
- [ ] 7.3 Send run-start replies for accepted long-running workflow runs
- [ ] 7.4 Send final result replies when email-triggered workflow runs complete
- [ ] 7.5 Send sender-suitable error replies when email-triggered workflow runs fail
- [ ] 7.6 Suppress or limit outgoing replies when reply-loop risk is detected

## 8. Verification

- [ ] 8.1 Add unit tests for account address lookup, sender authorization, and email-enabled workflow filtering
- [ ] 8.2 Add unit tests for deterministic workflow resolution and LLM inference fallback behavior
- [ ] 8.3 Add unit tests for confirmation, rejection, expiration, and missing-input interaction state transitions
- [ ] 8.4 Add unit tests for input extraction and validation outcomes
- [ ] 8.5 Add integration tests for inbound email persistence, attachment references, deduplication, workflow handoff, and reply sending
- [ ] 8.6 Add tests for auto-reply detection and reply-loop suppression
