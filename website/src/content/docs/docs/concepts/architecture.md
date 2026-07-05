---
title: Architecture
description: Learn how RunHelm separates orchestration, task execution, and operator visibility.
---

RunHelm separates control-plane concerns from execution concerns.

## Orchestrator

The Rust orchestrator owns workflow definitions, run state, scheduling, and status APIs.

Key responsibilities include:

- registering workflow definitions
- creating workflow instances
- tracking task and run state
- selecting runnable tasks
- dispatching work through executor ports
- exposing HTTP endpoints for status and workflow operations

The current default wiring uses in-memory storage, an in-memory workflow queue, and a worker-pool-backed executor path.

## Worker

The TypeScript worker runtime executes individual task payloads.

Workers:

- register with the orchestrator worker API
- claim task payloads
- select an executor through `ExecutorFactory`
- execute agent, API-call, or function tasks
- validate task output against JSON Schema
- read required credentials through a credentials port

Function tasks run arbitrary code in an isolated Node.js child process. Agent tasks use a provider-agnostic model interface, approved tools, selected skills, and credentials loaded through the worker credential adapter.

## Frontend

The frontend is the start of an operator UI for workflows, runs, and system visibility. It is currently a shell rather than a fully integrated console, but it establishes the direction for observing orchestrated runs.

## Ports and adapters

Side effects live behind ports and adapters. Storage, workflow queues, executors, credentials, and worker dispatch are modeled as replaceable boundaries so the core orchestration logic can remain testable and cohesive.
