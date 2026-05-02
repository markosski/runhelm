
# Design & Architecture

## Capabilities

- A single workflow is composed of tasks. Each task is a unit of work that can be either:
  - task can be of type "agent" or "function"
    - function type are more generic, where it can execute arbitrary code including integration with external services, e.g. calling API
  - Code that takes well defined input, executes arbitrary code (including handling temporary files) in a fully isolated ephemeral environment, and produces well defined output.
  - An LLM agent task with a provided prompt that takes input as context, utilizes a provider-agnostic interface (swap models via API keys), and produces well defined output.
- Workflows are defined in JSON to support future UI workflow designer initiatives.
- Workflows can be paused, resumed and restarted on task failure.
- Agent tasks can use tools, some of which will be built-in (e.g. web search, api call, rdbms database call etc.) but tools can also be installed by the user by providing npm dependencies.
- Same workflows can execute concurrently as defined by the workflow owner.

## API

- ability to trigger specific task in the workflow in isolation to test it
- ability to check status - which task are we on now, completed|failed|running|pending
- ability to manually pasuse and resume - pausing should not interrupt current task, it should let it complete and pause at that point

## Architecture Setup

The system enforces separation of concerns between an **Orchestrator** (the control plane tracking progress and scheduling tasks) and **Workers** (the ephemeral execution environment, targeting AWS Lambda for MVP).

## Runs**
- each run metadata is recorded in storage (pluggable backends)
- runs contain information about workflow inputs and outputs

## State & Persistence

State persistence, workflow definitions, and the internal durable queue for tasks awaiting processing will leverage AWS services (e.g., DynamoDB) where possible.

## Integrations 

Integrations (e.g., Kafka, SQS, RDBMS, Mailgun) will be facilitated through the Orchestrator API. Users will stand up their own infrastructure pieces that either poll from or submit data to RunHelm's API.

## Task Components

- Docker Container Images - User provided docker images
- Schemas - Input / Output schema definitions
  - Schemas may contain constant values, e.g. imagine API for mailgun, where schema will define from, to, subject, title, content fields, where from and subject fields may be static values and remaining fields will change based on the data
- LLM Agent Prompts - agent instructions, requires schemas and containers as input to execute
- APIs - definition of an API, requires I/O schemas
- Functions - code, requires container image
- Workflows - flow defined as DAG
  - Workflow should be a DAG or something similar to it. There needs to be a concept of bounded cycles. Why? Image setup where there are several task in a sequence A (make code change) -> B(review), this cycle may repeast until review task is satisfied. This does not mean we need a Cyclic Graph, we may still represent this is Acyclic Graph just that the cycles would need to materialize into repeated task sequences with short circuit to break out to exit "cycle" early. For example A -> B -> C -> B -> C -> E, where if C is satisfies it should short circuit, skip second B -> C cycle and move on to E.

## Decision Records

**Model provider:**
- Start with BYOK concepts, use any provider that allos to use common API req/res model, e.g. OpenAI API

**Programming languages:**
- Orchestrator is developed using Rust, leveraging strong concurrency model using Tokio
- Workers are code deployed in containers with RunHelm TypeScript package, leveraging its dynamic nature of code execution and package distribution for extensions

**Worker Backends:** 
- Workers are executed on different backends, e.g. AWS Lambda, AWS Fargate with container isolation. Orchestrator worker interface allows to dispatch to different worker backends. For the MVP we will try to target AWS Lambda as a first backend - negotiable, goal is to first pick easiest to deploy on compute backend.