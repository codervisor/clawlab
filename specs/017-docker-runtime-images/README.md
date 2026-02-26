---
status: planned
created: 2026-02-26
priority: high
tags:
- docker
- deployment
- runtime
- infra
- container
parent: 009-orchestration-platform
depends_on:
- 010-claw-runtime-interface
created_at: 2026-02-26T02:42:25.266699500Z
updated_at: 2026-02-26T02:42:25.266699500Z
---

# Docker Runtime Images & Deployment

## Overview

Each claw runtime needs a production-ready Docker image and deployment tooling so ClawLab can spin up, manage, and scale agent containers. The `zeroclaw-docker` repo (`~/projects/codervisor/zeroclaw-docker`) establishes the reference pattern — multi-stage build, env-override entrypoint, Docker Compose, GHCR CI/CD. This spec generalizes that pattern into a repeatable template for every supported runtime and integrates it with ClawLab's CRI adapter layer.

## Context

### What `zeroclaw-docker` Already Provides (Reference Implementation)

| Artifact | Purpose |
|---|---|
| `Dockerfile` | Multi-stage: builder (compile from source) → runtime (minimal Debian slim) |
| `config.toml` | Default config baked in; every key overridable via env vars |
| `entrypoint.sh` | TOML patcher — env vars → config at container start, drops to non-root user via `gosu` |
| `docker-compose.yml` | Single-service compose with `.env` auto-load, volume mount, restart policy |
| `.env.example` | Comprehensive template of all env vars with defaults |
| `.github/workflows/docker.yml` | GitHub Actions: build + push to `ghcr.io/codervisor/<runtime>:latest` |
| `workspace/` | Scaffold files (identity, docs) copied into container |

### Why This Matters for ClawLab

ClawLab's CRI adapter layer (spec 010) needs actual running runtimes to talk to. The `install()` method on `ClawAdapter` must be able to pull images, configure them, and start containers. Standardizing the Docker image structure across runtimes makes adapter implementation predictable: same health endpoint convention, same env-var override pattern, same container user model.

## Design

### Generalized Runtime Image Template

Each runtime gets its own `-docker` repo following a common structure:

```
<runtime>-docker/
├── Dockerfile              # Multi-stage: build from source → slim runtime
├── config.<ext>            # Runtime-native config with sane defaults
├── entrypoint.sh           # Config patcher + privilege drop
├── docker-compose.yml      # Local dev / single-node deploy
├── .env.example            # All env overrides documented
├── workspace/              # Bootstrap files (identity, prompts, etc.)
└── .github/workflows/
    └── docker.yml          # Build → push to ghcr.io/codervisor/<name>
```

### Per-Runtime Specifics

| Runtime | Language | Config Format | Default Port | Image Base | Build Strategy |
|---------|----------|---------------|-------------|------------|----------------|
| ZeroClaw | Rust | TOML | 42617 | `rust:slim` → `debian:slim` | `cargo build --release` (reference impl exists) |
| OpenClaw | Python | JSON | 3000 | `python:slim` → `debian:slim` | `pip install` or clone + install |
| PicoClaw | Go | JSON | 8080 | `golang:alpine` → `alpine` | `go build` (static binary) |
| NanoClaw | Node.js | JSON | 7070 | `node:lts-slim` | `npm ci --production` |
| IronClaw | Rust | TOML | 9090 | `rust:slim` → `debian:slim` | `cargo build --release` |
| NullClaw | Rust | TOML | 6060 | `rust:slim` → `debian:slim` | `cargo build --release` |

### Common Conventions

1. **Non-root execution**: All containers run as a dedicated user (e.g., `zeroclaw:10001`, `openclaw:10002`) via `gosu`
2. **Health endpoint**: `GET /health` on the runtime's port — used by Docker `HEALTHCHECK` and ClawLab's `HealthMonitor`
3. **Config override pattern**: Baked-in default config + entrypoint patches values from env vars at startup (never mutates original)
4. **Volume mount**: `./data:/home/<user>/.<runtime>/workspace` — persistent workspace data
5. **GHCR registry**: All images at `ghcr.io/codervisor/<runtime>:latest` with optional semver tags
6. **Restart policy**: `unless-stopped` in compose; ClawLab RecoveryEngine handles higher-level restart logic
7. **Secrets**: API keys via env vars only (never baked into image); secrets encryption enabled by default in config

### Integration with ClawLab CRI

The `install()` method in each CRI adapter:
1. Pulls the Docker image (`ghcr.io/codervisor/<runtime>:<version>`)
2. Generates env overrides from ClawLab's canonical config (spec 013)
3. Creates a container with the appropriate port mapping, volume, and env
4. Starts the container and waits for `/health` to return 200

The `stop()`/`restart()` methods map directly to Docker container lifecycle.

`health()` calls `GET /health` on the container's mapped port.

### Config Translation at Deploy Time

```
ClawLab canonical config (TOML)
        │
        ▼
  CRI config translator (spec 013)
        │
        ▼
  Runtime-specific env vars
        │
        ▼
  entrypoint.sh patches config.<ext>
        │
        ▼
  Runtime reads native config
```

## Plan

- [ ] Extract reusable template from `zeroclaw-docker` (Dockerfile template, entrypoint pattern, compose template, CI workflow)
- [ ] Create `openclaw-docker` repo following the template — Python-based build, JSON config, port 3000
- [ ] Create `picoclaw-docker` repo following the template — Go static binary, JSON config, port 8080
- [ ] Create `nanoclaw-docker` repo following the template — Node.js, JSON config, port 7070
- [ ] Create `ironclaw-docker` repo following the template — Rust build, TOML config, port 9090
- [ ] Create `nullclaw-docker` repo following the template — Rust build, TOML config, port 6060
- [ ] Add `docker-compose.fleet.yml` to ClawLab for bringing up the full fleet locally (all runtimes)
- [ ] Implement Docker-based `install()` / `start()` / `stop()` in each CRI adapter (connects to spec 010)
- [ ] Document the runtime image template in ClawLab developer docs

## Test

- [ ] Each runtime image builds successfully from its Dockerfile
- [ ] Each container starts, passes `/health` check, and accepts API requests
- [ ] Env var overrides correctly patch the runtime config at startup
- [ ] Containers run as non-root and cannot write outside their workspace
- [ ] `docker-compose.fleet.yml` brings up at least 3 runtimes with ClawLab connecting to all
- [ ] CI workflow pushes images to GHCR on merge to main
- [ ] ClawLab CRI `install()` can pull and start a container end-to-end

## Notes

- `zeroclaw-docker` is the reference implementation — copy its patterns, don't abstract prematurely
- Runtimes that don't have a gateway yet (e.g., NanoClaw filesystem IPC) may need a sidecar HTTP shim until their upstream adds an HTTP API
- Consider a `clawlab runtime init <name>` CLI command that scaffolds a new `-docker` repo from the template
- Multi-arch builds (amd64 + arm64) are a stretch goal — start with amd64 only
- Image size matters for fleet scale-up; track and optimize layer caching
