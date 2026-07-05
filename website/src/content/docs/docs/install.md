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
