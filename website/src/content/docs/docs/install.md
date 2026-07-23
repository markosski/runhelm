---
title: Install RunHelm Locally
description: Run RunHelm locally with Docker and the runhelm wrapper.
---

The Docker-first local install path does not require Rust, Node.js, or a source checkout after installation. It uses prebuilt images by default and manages local config under `~/.runhelm`.

```bash
curl -fsSL https://raw.githubusercontent.com/markosski/runhelm/main/packaging/install.sh | sh
runhelm init
runhelm up
runhelm status
```

## Local files

`runhelm init` creates local state under:

```text
~/.runhelm/
  config.env
  file_credentials.json
  docker-compose.yml
  cache/
  skills/
  workspaces/
  workflows/
```

The generated config is written to `~/.runhelm/config.env`, and the generated Compose file is written to `~/.runhelm/docker-compose.yml`.

## Default namespace

Public resource endpoints require a namespace. Configure a canonical UUID string for local and single-tenant deployments:

```text
RUNHELM_DEFAULT_NAMESPACE=550e8400-e29b-41d4-a716-446655440000
```

The namespace resolver checks and validates this value when resolving each public resource request. The configured default is authoritative, so public requests do not need an authorization header. If it is absent or empty, public resource requests require a bearer API key; API-key-to-namespace resolution is not implemented yet. Health checks remain available without namespace configuration or authorization.

The repository's local-development `docker-compose.yml` supplies this example namespace to the orchestrator service by default.

## Image overrides

Override image references in `~/.runhelm/config.env` when using an internal registry:

```text
RUNHELM_ORCHESTRATOR_IMAGE=registry.example.com/runhelm-orchestrator:0
RUNHELM_WORKER_IMAGE=registry.example.com/runhelm-worker:0
RUNHELM_FRONTEND_IMAGE=registry.example.com/runhelm-frontend:0
```

## Self-build path

Users who need to own their image artifacts can build them from a checkout or git ref:

```bash
packaging/build-images.sh --ref v0.3.1 --tag-prefix registry.example.com/runhelm --push
```

Use the source-build path for contributor workflows or controlled image publishing. The normal local-user path should stay Docker-first and use published images.
