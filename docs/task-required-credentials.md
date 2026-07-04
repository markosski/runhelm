# Task Required Credentials

`required_credentials` lists the named secrets a task needs before it can run.
The worker reads each name through the configured `CredentialsPort`.

During task execution, the worker also exposes every required credential as an
uppercased environment variable:

```yaml
required_credentials:
  - llm_api_key
  - gh_token
```

With the file credential adapter, the worker reads those names from
`~/.runhelm/file_credentials.json`:

```json
{
  "llm_api_key": "...",
  "gh_token": "github_pat_..."
}
```

Agent tasks use the first required credential as the model API key, preserving
the existing Agent credential convention. The full required credential set is
available to approved tools executed by the Agent, including shell tools. For
example, `gh_token` is available as `GH_TOKEN`.

Function tasks receive required credentials in the function context and in the
child process environment.

If any required credential is missing, the task fails before its main work runs.
