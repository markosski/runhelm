---
title: Human Input Workflow
description: A workflow that pauses in InputNeeded and continues after an operator response.
---

This example shows an Agent task that intentionally asks the operator for a missing release channel before returning structured output.

It is based on `worker/examples/example_human_input_workflow.yaml`.

## Workflow definition

```yaml
id: human-input-agent-workflow

tasks:
  - id: collect-release-preference
    kind:
      Agent:
        model_id: "google/gemini-2.5-flash"
        provider_url: ""
        prompt: |
          You are preparing a release note summary, but the release channel is
          intentionally missing.

          If there is no USER RESPONSE TO PREVIOUS INQUIRY in the current task
          context, call the ask_user tool with this exact question:
          "Which release channel should this summary target: stable, beta, or nightly?"

          If a USER RESPONSE TO PREVIOUS INQUIRY is present, use that response
          as the release channel and return exactly this JSON shape:
          {
            "response": "Release summary prepared for <channel> channel.",
            "channel": "<channel>"
          }
        tools: ["ask_user"]
        skills: []
        ask: true
        schema_failure_retry_times: 2
        reuse_session: true
    output_schema:
      type: object
      required:
        - response
        - channel
      properties:
        response:
          type: string
        channel:
          type: string
    required_credentials:
      - llm_api_key

data_bindings: []
```

## Configure credentials

Add the model credential to `~/.runhelm/file_credentials.json`:

```json
{
  "llm_api_key": "..."
}
```

## Register and start

Register the workflow with the API. If you are working from the repository checkout, you can convert the example YAML to JSON with your preferred YAML tool and post it to `/workflow-def`.

Start an instance:

```bash
export RUNHELM_URL=http://localhost:3000

curl -sS -X POST "$RUNHELM_URL/workflow-def/human-input-agent-workflow" \
  -H 'content-type: application/json' \
  -d '{}'
```

The run should eventually move to `InputNeeded`.

## Inspect the question

Read the task result:

```bash
curl -sS "$RUNHELM_URL/workflows/human-input-agent-workflow-1780000000000000000/tasks/collect-release-preference"
```

Example response:

```json
{
  "status": "input_needed",
  "input": [],
  "input_request": "Which release channel should this summary target: stable, beta, or nightly?",
  "task_def_id": "collect-release-preference",
  "task_attempt_id": "collect-release-preference[1]",
  "satisfaction": "Unsatisfied",
  "generation_index": 1
}
```

## Submit the answer

```bash
curl -sS -X POST "$RUNHELM_URL/workflows/human-input-agent-workflow-1780000000000000000/tasks/collect-release-preference/human-input" \
  -H 'content-type: application/json' \
  -d '{ "input": "stable" }'
```

Response:

```json
{
  "status": "queued",
  "workflow_instance_id": "human-input-agent-workflow-1780000000000000000",
  "task_attempt_id": "collect-release-preference[2]"
}
```

## Final result

After the continuation runs, read the task result again:

```bash
curl -sS "$RUNHELM_URL/workflows/human-input-agent-workflow-1780000000000000000/tasks/collect-release-preference"
```

Example output:

```json
{
  "status": "success",
  "input": [],
  "output": {
    "response": "Release summary prepared for stable channel.",
    "channel": "stable"
  },
  "task_def_id": "collect-release-preference",
  "task_attempt_id": "collect-release-preference[2]",
  "satisfaction": "Satisfied",
  "generation_index": 2
}
```

See [Human Input](/docs/concepts/human-input/) for the full behavior and design guidance.
