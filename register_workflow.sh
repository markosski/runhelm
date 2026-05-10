#!/bin/bash

yq . worker/example_workflow.yaml \
| curl -sS -X POST http://localhost:3456/workflow-def \
-H 'Content-Type: application/json' \
--data-binary @-
