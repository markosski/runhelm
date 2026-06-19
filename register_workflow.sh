#!/bin/bash

yq . worker/examples/example_workflow.yaml \
| curl -sS -X POST http://localhost:3000/workflow-def \
-H 'Content-Type: application/json' \
--data-binary @-
