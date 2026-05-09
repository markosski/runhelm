## Context

RunHelm already supports workflow execution through schedules and API calls. Email adds a human-facing trigger channel where a user can send a natural-language request, include attachments, and receive status or results in the same thread.

The preferred product model is one memorable inbound email address per account. Workflow routing should be deterministic when possible, but can use an LLM to infer intent when the message does not explicitly name a workflow. Because email content and sender headers are untrusted inputs, authorization, workflow eligibility, confirmation, and input validation must remain system decisions rather than LLM decisions.

## Goals / Non-Goals

**Goals:**

- Support an account-level inbound email address as a workflow trigger source.
- Normalize provider-specific inbound email events into a RunHelm-owned message model.
- Resolve workflows with a deterministic-first pipeline: exact workflow handle/name, configured aliases, then LLM inference.
- Require explicit sender authorization before execution.
- Require confirmation for LLM-inferred workflow selection and for workflows whose policy requires confirmation.
- Validate extracted inputs against the selected workflow input schema before execution.
- Persist inbound messages and interaction state for idempotency, auditability, confirmation, missing-input collection, and reply handling.
- Reply to the original email thread with confirmations, prompts, run-start notifications, results, and errors.

**Non-Goals:**

- Running a first-party SMTP server.
- Supporting customer-owned mailboxes through Gmail, Microsoft Graph, or IMAP in the initial implementation.
- Treating the LLM as an authorization or policy enforcement mechanism.
- Making every workflow email-callable by default.
- Designing a complete natural-language workflow discovery assistant beyond the workflow resolution needed for email triggers.

## Decisions

### Use an email provider adapter instead of direct SMTP hosting

Inbound email should enter RunHelm through an `EmailProvider` adapter that verifies provider authenticity, exposes normalized message data, stores or streams raw content, and supports sending replies.

Initial implementation can target one provider, but core workflow email logic should depend on interfaces such as:

- `InboundEmailVerifier` for provider webhook verification.
- `InboundEmailNormalizer` for converting provider payloads into normalized messages.
- `EmailReplySender` for threaded replies.
- `EmailAttachmentStore` for storing attachments before workflow execution.

Alternatives considered:

- Direct SMTP hosting: gives full control but adds deliverability, abuse, parsing, and operational complexity too early.
- Gmail/Microsoft mailbox polling: useful for enterprise bring-your-own-mailbox later, but worse for platform-owned account addresses.

### Model email as a workflow trigger source

Email should hand off to the same workflow execution layer as schedules and APIs by creating a workflow run request with trigger metadata:

- trigger source: `email`
- account id
- sender identity
- inbound message id
- selected workflow id
- validated inputs
- attachments or attachment references
- reply thread metadata

The workflow engine should not need provider-specific email details.

Alternatives considered:

- Execute workflows inside the inbound webhook handler: simpler at first, but makes retries, long-running workflows, and failure handling harder.
- Create a separate email-only execution path: duplicates authorization, observability, and run lifecycle behavior.

### Persist inbound messages before processing

The inbound webhook should persist a normalized message record and a dedupe key before workflow resolution or execution. Attachments should be stored and referenced rather than kept only in memory.

This supports provider retries, replay/debugging, audit logs, large attachments, and asynchronous processing.

The model should distinguish:

- `EmailInboundMessage`: normalized headers, recipients, subject, body, provider ids, dedupe keys, and processing status.
- `EmailAttachment`: metadata and storage references.
- `EmailInteraction`: pending workflow selection, confirmation, missing input, run id, status, and expiration.

Alternatives considered:

- Process directly from webhook payload without storage: lower initial code, but fragile under retries and hard to audit.

### Resolve workflows deterministically before LLM inference

Workflow resolution should use a layered resolver:

1. Account address maps the message to an account.
2. Sender authorization determines whether the sender can use email triggers for that account.
3. Candidate workflows are limited to workflows explicitly enabled for email.
4. Exact workflow handle/name matches are attempted against subject and body.
5. Configured aliases are matched.
6. If no deterministic match exists, an LLM selects candidate workflows from limited workflow metadata.

The LLM should return structured output with a workflow id candidate, confidence, extracted rationale, and possible alternatives. The system should apply policy to decide whether to ask for confirmation, request disambiguation, or reject the message.

Alternatives considered:

- LLM-first routing: better natural language behavior but less deterministic, less debuggable, and more costly.
- Workflow-specific addresses only: more deterministic, but worse for memorability and less aligned with the account-level assistant-like email experience.

### Treat confirmation and follow-up replies as state transitions

When RunHelm asks for confirmation or missing inputs, it should create or update an `EmailInteraction` record tied to the email thread and sender. A short reply such as "yes" should resume the pending interaction rather than run full workflow inference again.

Interactions should expire to avoid stale approvals. Confirmation tokens or exact reply destinations can be added later if reply-thread matching is not reliable enough.

Alternatives considered:

- Re-run inference for every reply: simple, but ambiguous and unsafe for short replies.
- Require users to include full commands in every reply: more deterministic, but poor email ergonomics.

### Keep authorization and safety policies outside the LLM

Authorization should be enforced by RunHelm policy before workflow execution. Policy should consider:

- sender allowlists or domain allowlists
- account-level email trigger settings
- workflow-level email enablement
- workflow-level confirmation requirement
- workflow side-effect level

LLM output may influence which workflow is proposed, but it must not grant access, bypass confirmation, or decide that a sensitive operation is safe.

Alternatives considered:

- Ask the LLM to decide if a sender can run a workflow: unacceptable because authorization needs deterministic policy and auditability.

### Reply asynchronously for long-running workflows

For workflows that do not complete quickly, RunHelm should send an accepted/run-started reply and later send completion or failure results to the same thread. Synchronous webhook response timing should not determine the workflow execution lifecycle.

Alternatives considered:

- Wait for workflow completion before replying: works only for fast jobs and risks provider timeouts.

## Risks / Trade-offs

- Ambiguous workflow requests -> Ask for confirmation or present a small numbered choice list instead of guessing.
- Spoofed or forwarded email -> Require sender authorization and avoid using sender address alone for sensitive workflows without confirmation.
- Provider duplicate delivery -> Deduplicate using provider ids and message headers before creating runs.
- Prompt injection in email body or attachments -> Treat email content as untrusted input; restrict LLM tools/context and enforce policy outside the model.
- Long or large emails and attachments -> Store attachments separately, enforce size limits, and pass references to workflows.
- Auto-reply loops -> Detect common auto-reply headers and suppress workflow triggering or reply sending.
- Stale confirmations -> Expire pending interactions and require a new request after expiration.
- LLM misclassification -> Prefer exact matches, require confirmation for inferred selections, and log resolver decisions for audit/debugging.

## Migration Plan

1. Add data models and interfaces for inbound email messages, attachments, account email settings, sender authorization policy, and email interactions.
2. Implement one provider adapter behind the email provider interfaces.
3. Add an inbound email webhook that verifies provider authenticity, persists normalized messages, stores attachments, and enqueues processing.
4. Implement deterministic workflow matching, LLM inference, confirmation policy, and input extraction/validation.
5. Add email trigger handoff into the workflow execution entrypoint.
6. Add reply sending for confirmation, missing inputs, accepted runs, final results, and errors.
7. Enable email triggering behind account/workflow feature flags.

Rollback should disable account email trigger settings and stop accepting provider webhooks while keeping stored message records for audit.

## Open Questions

- Which inbound email provider should be used for the initial adapter?
- What exact account address format should be exposed publicly?
- Should exact workflow name matches execute immediately for all read-only workflows, or should all first-run email triggers require confirmation?
- What attachment size and content-type limits should the MVP enforce?
- How should workflow results be summarized when outputs are large or contain sensitive data?
