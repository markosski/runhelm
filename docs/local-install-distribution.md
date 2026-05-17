# Local Install Distribution

## Goal

RunHelm should be easy to run locally without requiring users to build the codebase from source. The expected prerequisite for local users is Docker with Compose support. Users should not need Rust, Node.js, or knowledge of the repository layout.

The target experience is:

```bash
curl -fsSL https://runhelm.dev/install.sh | sh
runhelm init
runhelm up
```

## Recommended Approach

Use prebuilt Docker images as the primary distribution artifact.

Each releasable RunHelm version should publish runtime images for the main services:

```text
ghcr.io/runhelm/orchestrator:0.3.1
ghcr.io/runhelm/worker:0.3.1
ghcr.io/runhelm/frontend:0.3.1
```

The user-facing install flow should generate a local Compose file that references those images instead of building from local source. This keeps installation fast, reproducible, and independent of local build tooling.

## Local CLI

Provide a small `runhelm` CLI or wrapper that manages the local Docker-based installation.

Initial commands:

```bash
runhelm init
runhelm up
runhelm down
runhelm status
runhelm logs
runhelm update
runhelm doctor
```

The CLI should create and manage local state under:

```text
~/.runhelm/
  config.env
  credentials.json
  docker-compose.yml
  workflows/
```

`runhelm doctor` should verify:

- Docker is installed.
- The Docker daemon is running.
- `docker compose` is available.
- Required ports are available.
- The local RunHelm config directory exists.
- Required config and credential files exist or have clear setup guidance.

## Compose Files

Keep the repository's development Compose file separate from the release Compose file.

The development file can continue to build from local source:

```yaml
services:
  orchestrator:
    build:
      context: ./orchestrator
```

The release/local-user Compose file should use published images:

```yaml
services:
  orchestrator:
    image: ghcr.io/runhelm/orchestrator:${RUNHELM_VERSION}
    ports:
      - "3000:3000"

  worker:
    image: ghcr.io/runhelm/worker:${RUNHELM_VERSION}
    environment:
      RUNHELM_ORCHESTRATOR_HTTP_URL: http://orchestrator:3000

  frontend:
    image: ghcr.io/runhelm/frontend:${RUNHELM_VERSION}
    ports:
      - "5173:80"
```

The generated local Compose file should pin exact versions by default, for example `0.3.1`, so a user's local install does not change behavior just because a moving tag was updated.

## Image Versioning

Publish immutable exact version tags and optional moving channel tags:

```text
ghcr.io/runhelm/orchestrator:0.3.1
ghcr.io/runhelm/orchestrator:0.3
ghcr.io/runhelm/orchestrator:0
ghcr.io/runhelm/orchestrator:latest
```

Exact version tags should be the default in generated Compose files. Moving tags are useful for convenience and testing, but local installs should be stable unless the user explicitly updates.

`runhelm update` should rewrite the generated Compose file to a newer exact version and then pull/restart the services.

## Source Build Path

A source-build flow is still useful, but it should be treated as a contributor or advanced-user path.

Development:

```bash
git clone https://github.com/runhelm/runhelm.git
cd runhelm
docker compose up --build
```

Optional future advanced command:

```bash
runhelm build --ref main
runhelm up --local
```

This should not be the default install path. Building from source during install is slower, depends on network and local build cache behavior, and makes support harder because each user produces their own runtime artifact.

## Decision

The primary local distribution model is Docker-first with prebuilt, versioned service images. Every release should publish images through CI. Users run those images through a generated Compose file managed by the `runhelm` CLI.

Source builds remain available for contributors and advanced testing, but normal users should only need Docker.
