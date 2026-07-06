---
title: Task Credentials
description: Configure the credentials tasks need before execution.
---

`required_credentials` lists the named secrets a task needs before it can run. The worker reads each name through the configured credentials port.

```yaml
required_credentials:
  - llm_api_key
  - gh_token
```

With the file credential adapter, the worker reads those names from `~/.runhelm/file_credentials.json`:

```json
{
  "llm_api_key": "...",
  "gh_token": "github_pat_..."
}
```

## Runtime exposure

During task execution, the worker exposes every required credential as an uppercased environment variable. For example, `gh_token` becomes `GH_TOKEN`.

Function tasks receive required credentials in the function context and in the child process environment.

Agent tasks use the first required credential as the model API key, preserving the current agent credential convention. The full required credential set is available to approved tools executed by the agent.

## Missing credentials

If any required credential is missing, the task fails before its main work runs. This keeps credential failures explicit and avoids starting work that cannot complete.
