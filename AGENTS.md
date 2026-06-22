# Runhelm Project

This is instructions to the agent on how to do work in this project.

## Project Description

RunHelm is a agentic workflow orchestrator. It focuses on composing workflows from agents and tools, and executing them in a reliable and observable way.

## Important!
- Always update documents in docs/ directory with most recent changes made
- Focus on minimum or reasonable necessary code changes and functionality, i.e. do not add code to future proof design
- When making changes to existing code favor simplicity over backwards compatibility
- Do not make assumptions about critical decisions, ask to clarify underspecified information
- When creating data contracts between modules, APIs, etc. don't future proof models, create models that only include information that is needed and not what may be needed in the future

## Design Principles
- TypeScript is prefered to be used due to type safety, wide adoption, ease of integration and dynamic nature (executing arbitrary task code) .
- Code is organized into highly cohesive modules for ease of maintenance and testing.
- Components that perform side effects, e.g. persisting data, making network calls are exposed behind interfaces for testability and plugability.
- Code that does not perform any side effects is organized as functions, optionally taking component interface if they need to perform a side effect.