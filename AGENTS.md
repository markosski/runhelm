# Project Description

RunHelm is a agentic workflow orchestrator. It focuses on composing workflows from agents and tools, and executing them in a reliable and observable way.

# Design Principles
- TypeScript is prefered to be used due to type safety, wide adoption, ease of integration and dynamic nature (executing arbitrary task code) .
- Code is organized into highly cohesive modules for ease of maintenance and testing.
- Components that perform side effects, e.g. persisting data, making network calls are exposed behind interfaces for testability and plugability.
- Code that does not perform any side effects is organized as functions, optionally taking component interface if they need to perform a side effect.