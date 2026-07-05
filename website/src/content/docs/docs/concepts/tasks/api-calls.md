---
title: API Call Tasks
description: Use direct API calls for simple HTTP-style workflow steps.
---

API call tasks represent direct service calls in a workflow. Use them when a step can be expressed as a request without model reasoning or custom JavaScript.

```yaml
tasks:
  - id: fetch-status
    kind:
      ApiCall:
        url: "https://example.com/status"
        method: "GET"
    required_credentials: []
```

## When to use API call tasks

Use an API call task when:

- the request shape is simple
- the result can flow directly into downstream data bindings
- the workflow does not need SDK-specific behavior
- a Function task would only wrap one straightforward request

Use a Function task instead when the step needs request signing, provider SDKs, pagination, response normalization, retries with provider-specific behavior, or file output.

## Contracts

As with other task kinds, declare `input_schemas` and `output_schema` when downstream behavior depends on a specific shape. The schema is the boundary that makes API responses safe to consume later in the workflow.
