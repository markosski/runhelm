# Runhelm Project

This is instructions to the agent on how to do work in this project.

## Project Description

* RunHelm is a agentic workflow orchestrator. It focuses on composing workflows from agents and tools, and executing them in a reliable and observable way.

## Github integration

* This project is maintained in private repo at this URL -> https://github.com/markosski/runhelm
* User may request for agent to implement specific Github issue, in this case use built in capabilities or command line tools to lookup Github issue content and work with the user to implement it.
* User may request for agent to create a new Github issue based on the recent discussion. Agent should use it's capabilities like Github plugin or command line tools to create the issue with well describe content and acceptance criteria. Issue should contain following sections:
    * Problem - descxribe issue and provide scenario explainer
    * Goal - what is the intended behavior we want
    * Acceptance Criteria - list of tasks to meet the goal
    * Notes - additional information or context that will help with implementation

## Important!
- Always update existing website documentation with most recent changes made, contents are located in website/ directory, using Starling framework for documentation hosting.
- Focus on minimum or reasonable necessary code changes and functionality, i.e. do not add code to future proof design
- When making changes to existing code favor simplicity over backwards compatibility
- Do not make assumptions about critical decisions, ask to clarify underspecified information
- When creating data contracts between modules, APIs, etc. don't future proof models, create models that only include information that is needed and not what may be needed in the future

## Design Principles
- Code is organized into highly cohesive modules for ease of maintenance and testing.
- Components that perform side effects, e.g. persisting data, making network calls are exposed behind interfaces for testability and plugability.
- Code that does not perform any side effects is organized as functions, optionally taking component interface if they need to perform a side effect.
- Refrain from duplicating logic in multiple places