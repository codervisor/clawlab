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

| Runtime | Repo | Language | Config Format | Default Port | Image Base | Build Strategy |
|---------|------|----------|---------------|-------------|------------|----------------|
| ZeroClaw | `zeroclaw-labs/zeroclaw` | Rust | TOML | 42617 | `rust:slim` → `debian:slim` | `cargo build --release` (reference impl exists) |
| OpenClaw | `openclaw/openclaw` | TypeScript (Node.js ≥22) | JSON | 18789 | `node:22-slim` | `pnpm install && pnpm build` |
| PicoClaw | `sipeed/picoclaw` | Go | JSON | — | `golang:alpine` → `alpine` | `go build` (static binary) |
| NanoClaw | `qwibitai/nanoclaw` | TypeScript (Node.js 20+) | code-driven | — | `node:20-slim` | `npm install` + Claude Agent SDK |
| IronClaw | `nearai/ironclaw` | Rust | env/DB (PostgreSQL + pgvector) | — | `rust:slim` → `debian:slim` | `cargo build --release` |
| NullClaw | `nullclaw/nullclaw` | Zig | JSON | 3000 | custom (Zig 0.15.2 builder) → `debian:slim` | `zig build -Doptimize=ReleaseSmall` (678 KB static binary) |
| MicroClaw | `microclaw/microclaw` | Rust | YAML | — | `rust:slim` → `debian:slim` | `cargo build --release` |

**Not containerizable (embedded-only):**
- **MimiClaw** (`memovai/mimiclaw`) — pure C on ESP32-S3, bare-metal firmware. Requires ESP-IDF toolchain + physical hardware. ClawLab will support it via a serial/MQTT bridge adapter rather than Docker.

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
- [ ] Create `openclaw-docker` repo — Node.js 22 build, JSON config, gateway port 18789
- [ ] Create `picoclaw-docker` repo — Go static binary build, JSON config
- [ ] Create `nanoclaw-docker` repo — Node.js 20 + Claude Agent SDK, code-driven config
- [ ] Create `ironclaw-docker` repo — Rust build, PostgreSQL + pgvector sidecar
- [ ] Create `nullclaw-docker` repo — Zig 0.15.2 builder, JSON config, port 3000 (678 KB binary)
- [ ] Create `microclaw-docker` repo — Rust build, YAML config
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
- **OpenClaw** is the largest ecosystem (229K+ stars, TypeScript/Node.js). Its gateway runs on port 18789 with WS control plane. Config is `~/.openclaw/openclaw.json`
- **PicoClaw** (Go, 20K+ stars) is ultra-lightweight (<10 MB RAM). Produces a static binary — ideal for minimal Alpine images. Already has Docker Compose in its repo
- **NanoClaw** (TypeScript, 15K+ stars) runs on Claude Agent SDK with container isolation (Apple Container / Docker). Config is code-driven, not file-based — may need a thin wrapper
- **IronClaw** (Rust, 3.5K+ stars) requires PostgreSQL + pgvector — Docker image needs a DB sidecar or external DB URL. Uses WASM sandbox for tool isolation
- **NullClaw** (Zig, 2.2K+ stars) produces a 678 KB static binary with ~1 MB RAM. Port 3000 by default. JSON config at `~/.nullclaw/config.json`
- **MicroClaw** (Rust, 410 stars) — channel-agnostic agentic assistant. YAML config
- **MimiClaw** (C, 3.3K+ stars) — bare-metal ESP32-S3 firmware, not containerizable. ClawLab supports it via serial/MQTT bridge adapter in the CRI layer
- Consider a `clawlab runtime init <name>` CLI command that scaffolds a new `-docker` repo from the template
- Multi-arch builds (amd64 + arm64) are a stretch goal — start with amd64 only
- Image size matters for fleet scale-up: NullClaw (678 KB) and PicoClaw (~8 MB) are ideal for rapid scale-up; OpenClaw (~28 MB dist) is the heaviest
