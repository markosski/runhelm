# Local Install Distribution

## Goal

RunHelm should be easy to run locally without requiring users to build the codebase from source. The expected prerequisite for local users is Docker with Compose support. Users should not need Rust, Node.js, or knowledge of the repository layout.

The target experience is:

```bash
curl -fsSL https://raw.githubusercontent.com/markosski/runhelm/main/packaging/install.sh | sh
runhelm init
runhelm up
```

## Distribution Approach

Use prebuilt Docker images as the primary distribution artifact, with a documented self-build path for users who want to own or mirror the image artifacts.

Each releasable RunHelm version publishes runtime images for the main services:

```text
ghcr.io/markosski/runhelm-orchestrator:0
ghcr.io/markosski/runhelm-worker:0
ghcr.io/markosski/runhelm-frontend:0
```

The user-facing install flow generates a local Compose file that references those images instead of building from local source. This keeps installation fast, reproducible, and independent of local Rust or Node.js tooling.

Users who need internal registry control, audited builds, private base images, custom worker packages, or certificate injection can build images from a selected RunHelm source ref and override the image references in their generated config.

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
  file_credentials.json
  docker-compose.yml
  cache/
  skills/
  workspaces/
  workflows/
```

`packaging/runhelm` implements the initial wrapper. It is intentionally a small POSIX shell script so local installs do not require a compiled client binary. `packaging/install.sh` installs it into `~/.local/bin` by default, or into `RUNHELM_INSTALL_DIR` when that variable is set.

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
      dockerfile: Dockerfile.dev
```

The release/local-user Compose file should use published images:

```yaml
services:
  orchestrator:
    image: ${RUNHELM_ORCHESTRATOR_IMAGE}
    ports:
      - "3000:3000"
    environment:
      RUNHELM_WORKER_HTTP_ADDR: 0.0.0.0:3001
    healthcheck:
      test: ["CMD-SHELL", "wget -qO- http://127.0.0.1:3000/health >/dev/null && wget -qO- http://127.0.0.1:3001/health >/dev/null"]
      interval: 5s
      timeout: 2s
      retries: 12

  worker:
    image: ${RUNHELM_WORKER_IMAGE}
    depends_on:
      orchestrator:
        condition: service_healthy
    environment:
      RUNHELM_ORCHESTRATOR_HTTP_URL: http://orchestrator:3001
      RUNHELM_WORKER_HOST_ID: local-host
      RUNHELM_WORKSPACE_ROOT: /workspaces
    volumes:
      - ${RUNHELM_HOME}/file_credentials.json:/home/runhelm/.runhelm/file_credentials.json:ro
      - ${RUNHELM_HOME}/skills:/home/runhelm/.pi/agent/skills:ro
      - ${RUNHELM_HOME}/workspaces:/workspaces
      - ${RUNHELM_HOME}/cache:/home/runhelm/.cache

  frontend:
    image: ${RUNHELM_FRONTEND_IMAGE}
    ports:
      - "5173:80"
```

`packaging/docker-compose.release.yml` is the checked-in release Compose template. `runhelm init` writes a generated copy to `~/.runhelm/docker-compose.yml` and writes image references to `~/.runhelm/config.env`.

The generated config supports image overrides:

```env
RUNHELM_ORCHESTRATOR_IMAGE=ghcr.io/markosski/runhelm-orchestrator:0
RUNHELM_WORKER_IMAGE=ghcr.io/markosski/runhelm-worker:0
RUNHELM_FRONTEND_IMAGE=ghcr.io/markosski/runhelm-frontend:0
```

Users can point these values at an internal registry without changing the Compose file.

## Image Versioning

Publish immutable exact version tags and optional moving channel tags:

```text
ghcr.io/markosski/runhelm-orchestrator:0.3.1
ghcr.io/markosski/runhelm-orchestrator:0.3
ghcr.io/markosski/runhelm-orchestrator:0
ghcr.io/markosski/runhelm-orchestrator:latest
```

Exact version tags should be the default in generated Compose files. Moving tags are useful for convenience and testing, but local installs should be stable unless the user explicitly updates.

`runhelm update` should rewrite the generated Compose file to a newer exact version and then pull/restart the services.

The initial publishing workflow is `.github/workflows/publish-images.yml`. It publishes orchestrator, worker, and frontend images to GitHub Container Registry on version tags.

## Self-Build Path

The self-build path still performs Docker builds. It exists for users who want to control or mirror their image artifact, not for users who want to avoid building entirely. Users who want no build step should use the official images.

The repository includes:

```bash
packaging/build-images.sh --version dev --tag-prefix localhost/runhelm
packaging/build-images.sh --ref v0.3.1 --tag-prefix registry.example.com/runhelm --push
```

The script builds:

```text
<tag-prefix>/runhelm-orchestrator:<version>
<tag-prefix>/runhelm-worker:<version>
<tag-prefix>/runhelm-frontend:<version>
```

`--ref` builds from a local git ref using `git archive`. Without `--ref`, the script builds from the current checkout.

Custom worker images are expected to be common. The worker is where users are most likely to add OS packages, npm packages, internal tools, certificates, or private skill bundles. Those custom images can be used by setting `RUNHELM_WORKER_IMAGE` in `~/.runhelm/config.env`.

## Source Build Path

A source-build flow is still useful, but it should be treated as a contributor or advanced-user path.

Development:

```bash
git clone https://github.com/runhelm/runhelm.git
cd runhelm
docker compose up --build
```

For a faster orchestrator-only development loop:

```bash
scripts/run-orchestrator-dev.sh
scripts/run-orchestrator-dev.sh --skip-build
```

The script builds the debug binary unless `--skip-build` is provided, then runs `orchestrator/target/debug/orchestrator` directly without rebuilding Docker images.

For a matching single-worker loop:

```bash
scripts/run-worker-dev.sh
scripts/run-worker-dev.sh --skip-build
```

The script builds the worker TypeScript unless `--skip-build` is provided, then runs `worker/dist/index.js` directly against the local orchestrator worker API.

Optional future advanced command:

```bash
runhelm build --ref main
runhelm up --local
```

This should not be the default install path. Building from source during install is slower, depends on network and local build cache behavior, and makes support harder because each user produces their own runtime artifact.

## Decision

The primary local distribution model is Docker-first with prebuilt, versioned service images. Every release should publish images through CI. Users run those images through a generated Compose file managed by the `runhelm` CLI.

Source builds remain available for contributors and advanced testing, but normal users should only need Docker.

Durable local state currently covers worker workspaces, credentials, skills, and caches. Orchestrator workflow state still uses the configured orchestrator storage backend; the current default in-memory backend does not survive process or container restart. A durable storage backend is a separate reliability concern from the install path.
