#!/bin/bash

yq . worker/example_workflow.yaml \
| curl -sS -X POST http://localhost:3000/workflow-def \
-H 'Content-Type: application/json' \
-H "Authorization: Bearer ${RUNHELM_API_TOKEN:?Set RUNHELM_API_TOKEN to a token configured in RUNHELM_API_TOKENS}" \
--data-binary @-
