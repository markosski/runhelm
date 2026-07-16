#!/usr/bin/env bash

set -euo pipefail

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  echo "Usage: RUNHELM_AWS_REGION=<region> [RUNHELM_AWS_ENDPOINT_URL=<url>] $0"
  echo "Optional table overrides: RUNHELM_AWS_DEFINITIONS_TABLE, RUNHELM_AWS_WORKFLOW_INSTANCES_TABLE, RUNHELM_AWS_WORKFLOW_EVENTS_TABLE, RUNHELM_AWS_TASKS_TABLE"
  exit 0
fi

if ! command -v aws >/dev/null 2>&1; then
  echo "AWS CLI is required but was not found in PATH." >&2
  exit 1
fi

definitions_table="${RUNHELM_AWS_DEFINITIONS_TABLE:-runhelm-definitions}"
workflow_instances_table="${RUNHELM_AWS_WORKFLOW_INSTANCES_TABLE:-runhelm-workflow-instances}"
workflow_events_table="${RUNHELM_AWS_WORKFLOW_EVENTS_TABLE:-runhelm-workflow-events}"
tasks_table="${RUNHELM_AWS_TASKS_TABLE:-runhelm-tasks}"

aws_args=()
if [[ -n "${RUNHELM_AWS_REGION:-}" ]]; then
  aws_args+=(--region "${RUNHELM_AWS_REGION}")
fi
if [[ -n "${RUNHELM_AWS_ENDPOINT_URL:-}" ]]; then
  aws_args+=(--endpoint-url "${RUNHELM_AWS_ENDPOINT_URL}")
fi

create_table() {
  local table_name="$1"
  local describe_error

  if describe_error=$(aws "${aws_args[@]}" dynamodb describe-table --table-name "${table_name}" 2>&1); then
    echo "DynamoDB table already exists: ${table_name}"
    return
  fi
  if [[ "${describe_error}" != *"ResourceNotFoundException"* ]]; then
    echo "Unable to check DynamoDB table ${table_name}:" >&2
    echo "${describe_error}" >&2
    exit 1
  fi

  echo "Creating DynamoDB table: ${table_name}"
  aws "${aws_args[@]}" dynamodb create-table \
    --table-name "${table_name}" \
    --attribute-definitions \
      AttributeName=pk,AttributeType=S \
      AttributeName=sk,AttributeType=S \
    --key-schema \
      AttributeName=pk,KeyType=HASH \
      AttributeName=sk,KeyType=RANGE \
    --billing-mode PAY_PER_REQUEST \
    >/dev/null

  aws "${aws_args[@]}" dynamodb wait table-exists --table-name "${table_name}"
  echo "DynamoDB table is ready: ${table_name}"
}

create_table "${definitions_table}"
create_table "${workflow_instances_table}"
create_table "${workflow_events_table}"
create_table "${tasks_table}"

echo "RunHelm DynamoDB setup complete."
