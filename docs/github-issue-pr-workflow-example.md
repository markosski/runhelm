# GitHub Issue PR Workflow Example

`worker/examples/example_github_issue_pr_workflow.yaml` demonstrates an
agentic workflow that fetches a GitHub issue, implements the requested change,
uses a verifier loop to review the implementation, creates a pull request when
there are changes to publish, and comments back on the issue with the final
status.

## Worker Image Tooling

The production and development worker images install the GitHub CLI package
(`gh`) alongside `bash`, `curl`, and `git`.

The tasks that call `gh` list `gh_token` in `required_credentials`:

```yaml
required_credentials:
  - llm_api_key
  - gh_token
```

Add `gh_token` to the worker credential file together with `llm_api_key`:

```json
{
  "llm_api_key": "...",
  "gh_token": "github_pat_..."
}
```

The worker exposes each required credential as an uppercased environment
variable during task execution, so `gh_token` becomes `GH_TOKEN`. The GitHub
token must be able to read the target repository issue, add an issue comment,
push a branch, and create a pull request.

## Inputs

The first task requires one input object with the two issue identifiers:

```json
{
  "repository": "markosski/runhelm",
  "issue_number": 46
}
```

Current workflow definitions do not have a separate workflow-level input
schema. This example expresses the required inputs as the `fetch-issue` task
input schema so the contract is explicit in the workflow file. The orchestrator
persists the trigger request body as a single JSON value on the workflow
instance and attaches it to eligible root tasks when the engine materializes the
first task generation.

## Flow

1. `fetch-issue` uses `gh issue view` through the agent `bash` tool and returns
   structured issue details.
2. `implement-change` receives the issue details, updates the checkout in the
   shared `repo` workspace, runs relevant checks, and has `ask` enabled so it
   can pause for clarification when the issue is underspecified.
3. `review-implementation` is a verifier task. It checks the implementation
   against the issue criteria and test results. It can return `continue` with
   feedback, causing RunHelm to rerun from `implement-change`, for up to three
   implementation-review cycles.
4. `create-pull-request` runs after the verifier accepts the implementation.
   It commits and pushes a branch and creates the PR with `gh pr create` when
   there are changes. If the issue is already satisfied and there are no
   changes to commit, it returns `pr_created: false` with a short explanation.
5. `update-github-issue` always runs after the PR step and adds one concise
   issue comment. When a PR was created, the comment includes the PR number and
   URL. When no PR was needed, the comment summarizes why no PR was created.

All tasks use the same `workspace.group_name: repo` so files produced or edited
by one step are visible to later steps. The data bindings still define ordering;
sharing a workspace does not create dependencies by itself.
