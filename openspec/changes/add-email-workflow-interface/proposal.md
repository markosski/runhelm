## Why

RunHelm can currently be invoked through schedules and APIs, but those channels are not ideal for ad hoc, human-driven workflow requests. Adding email as an interaction surface lets account users trigger workflows from a familiar channel, provide natural-language context and attachments, and receive results in the same conversation.

## What Changes

- Add support for one inbound email address per account that can receive workflow requests.
- Resolve candidate workflows from inbound email by first matching exact workflow names or handles, then using an LLM to infer the intended workflow when no deterministic match is found.
- Require sender authorization before a workflow can be selected or executed.
- Ask the sender to confirm LLM-inferred workflow selections before execution.
- Extract workflow inputs from the subject, body, sender context, and attachments, then validate those inputs against the selected workflow's input requirements.
- Reply to the inbound email thread with confirmation requests, missing-input prompts, accepted/run-started messages, final results, or execution errors.
- Track pending email interactions so short replies such as "yes" or additional input can resume the correct workflow request.

## Capabilities

### New Capabilities

- `email-workflow-interface`: Account-level inbound email workflow triggering, workflow resolution, confirmation, input collection, execution handoff, and reply handling.

### Modified Capabilities

None.

## Impact

- Adds inbound email provider integration points for receiving parsed email webhooks and sending replies.
- Adds account email address configuration, sender authorization policy, and email-enabled workflow metadata.
- Adds persisted inbound message, attachment, and email interaction state for auditability, deduplication, confirmation, and follow-up replies.
- Adds workflow resolution and input extraction paths that combine deterministic matching, LLM inference, and schema validation.
- Affects workflow execution entrypoints by adding email as a new trigger source alongside schedules and API calls.
