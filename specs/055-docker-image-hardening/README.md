---
status: complete
created: 2026-03-06
priority: high
tags:
- docker
- security
- supply-chain
- infra
- container
- hardening
depends_on:
- 052-docker-image-runtime-binary-compat
parent: 017-docker-runtime-images
created_at: 2026-03-06T08:17:36.969275199Z
updated_at: 2026-03-06T08:56:01.682659274Z
---
# Docker Image Hardening — Security, Consistency & Supply-Chain Fixes

## Overview

A comprehensive audit of the ClawDen Docker runtime image (`docker/Dockerfile`, `docker/entrypoint.sh`, `docker/tools/`) revealed **1 critical, 6 high, and 14 medium** issues across security, supply-chain integrity, spec consistency, correctness, and UX.

All findings were resolved as part of the Docker distribution redesign that:
- Scoped the image to **OpenClaw and ZeroClaw only** (stable runtimes)
- Removed the tool activation system (all tools pre-installed in the image)
- Removed the sandbox layer (runtimes handle their own security; Docker provides isolation)
- Renamed the image from `clawden-runtime` to `clawden`
- Introduced per-runtime image tags (`:openclaw`, `:zeroclaw`, `:openclaw-browser`, etc.)

### Why Now

- **S1 (critical)**: Node.js was installed via `curl | bash` with no integrity verification — a supply-chain attack vector.
- **S2**: `PICOCLAW_VERSION=latest` and `OPENCLAW_VERSION=latest` broke reproducibility.
- **S3**: `nullclaw` appeared in `runtime_default_args()` but was not installed — dead code.
- **C1**: `core-utils` spec said `jq, yq, tree, file, zip/unzip, gzip` but `yq`, `file`, and `gzip` were never installed.

## Resolution

All items were resolved through the Docker distribution redesign. The redesign replaced the old multi-runtime, tool-activation architecture with a focused two-runtime design.

### Phase 1 — Critical & High Severity (all resolved)

#### 1. ~~Eliminate `curl | bash` for Node.js (S1)~~ ✅

Replaced with `COPY --from=node:22-bookworm-slim`. pnpm via `corepack enable pnpm`.

#### 2. ~~Pin all runtime versions (S2, S9)~~ ✅

Only OpenClaw and ZeroClaw remain. Both pinned:
```dockerfile
ARG OPENCLAW_VERSION=2026.3.2
ARG ZEROCLAW_VERSION=0.1.7
```

PicoClaw, NanoClaw, and OpenFang removed from the image (not stable enough).

#### 3. ~~Remove dead `nullclaw` case (S3)~~ ✅

Entrypoint rewritten — only `openclaw` and `zeroclaw` are recognized. No dead code.

#### 4. ~~Fix tool setup.sh scripts that call `sudo apt-get` (S4, S5)~~ ✅

Tool activation system removed entirely. All tools are pre-installed in the base image via apt-get. No setup scripts are sourced at container startup.

Note: `docker/tools/` directory with old manifests/scripts still exists on disk but is no longer referenced by the Dockerfile or entrypoint. Can be cleaned up in a follow-up.

#### 5. ~~Install missing `core-utils` components (C1)~~ ✅

`file` and `gzip` were added to apt-get, and the Go-based `yq` is copied from the pinned `mikefarah/yq:4.47.2` image.

#### 6. ~~Fix `$DEFAULT_ARGS` word-splitting (R1)~~ ✅

Entrypoint rewritten with explicit `exec "$LAUNCHER" gateway --allow-unconfigured` and `exec "$LAUNCHER" daemon --config-dir ...`. No variable expansion.

### Phase 2 — Medium Severity (all resolved)

#### 7. ~~Runtime-aware HEALTHCHECK (S6)~~ ✅

Each build target has its own `HEALTHCHECK` with the correct port:
- `:openclaw` → port 18789
- `:zeroclaw` → port 42617

#### 8. ~~Sandbox `/proc` exposure (S7)~~ ✅

Sandbox (bubblewrap) removed from the image. OpenClaw and ZeroClaw handle their own sandboxing. Docker itself provides process isolation.

#### 9. ~~Add signal trap in entrypoint (R2)~~ ✅

```bash
trap 'echo "[clawden] Interrupted"; exit 130' INT TERM
```

#### 10. ~~Validate env vars for all runtimes (R4)~~ — Descoped

Removed entrypoint-level validation. Runtimes handle their own API key validation and produce better error messages than a generic entrypoint check.

#### 11. ~~Fix tools.json output (C8)~~ ✅

Tool activation system removed. No `tools.json` is generated.

#### 12. ~~Spec parity fixes (C2, C3, C4, C6, C7)~~ ✅

Tool manifests and setup scripts are no longer used by the Dockerfile. The browser/gui empty directories are no longer created. All tools are pre-installed via apt-get.

#### 13. ~~Remove vestigial `browser`/`gui` empty directories (C7)~~ ✅

Removed from Dockerfile.

#### 14. ~~Eliminate redundant `apt-get update` for p7zip purge (R5)~~ ✅

`p7zip-full` no longer installed.

### Phase 3 — Low Severity & Optimization (all resolved)

#### 15. ~~Add `EXPOSE` directives (P4)~~ ✅

Per-target EXPOSE: `18789` for OpenClaw, `42617` for ZeroClaw, `6080` for computer variants.

#### 16. ~~Replace pnpm global install with corepack (P1)~~ ✅

```dockerfile
RUN ln -s ... && corepack enable pnpm
```

#### 17. ~~Reduce runtime list duplication (M1)~~ ✅

Only 2 runtimes. The runtime is set via `ENV RUNTIME=` in each build target — no duplication.

#### 18. ~~Add `phase` field to tool manifests (M2)~~ — Descoped

Tool manifests are not used at runtime. Deferred to if/when tool manifests are reintroduced.

## Plan

- [x] **Phase 1**: S1 — Replace `curl | bash` with `COPY --from=node:22-bookworm-slim`
- [x] **Phase 1**: S2/S9 — Pin OpenClaw and ZeroClaw versions; remove unstable runtimes
- [x] **Phase 1**: S3 — Remove dead code (entrypoint rewritten)
- [x] **Phase 1**: S4/S5 — Remove tool activation system; pre-install everything
- [x] **Phase 1**: C1 — Install `file`, `gzip` via apt-get
- [x] **Phase 1**: R1 — Fix `$DEFAULT_ARGS` word-splitting (explicit exec args)
- [x] **Phase 2**: S6 — Runtime-aware HEALTHCHECK port (per-target)
- [x] **Phase 2**: S7 — Remove sandbox (runtimes + Docker handle isolation)
- [x] **Phase 2**: R2 — Add signal trap to entrypoint
- [x] **Phase 2**: R4 — Descoped (runtimes validate their own env)
- [x] **Phase 2**: C8 — Removed tools.json (no tool activation)
- [x] **Phase 2**: C2/C3/C4/C6/C7 — Resolved by removing tool layer
- [x] **Phase 2**: R5 — Removed p7zip
- [x] **Phase 3**: P4 — Per-target EXPOSE directives
- [x] **Phase 3**: P1 — pnpm via corepack
- [x] **Phase 3**: M1 — No duplication (2 runtimes, ENV per target)
- [x] **Phase 3**: M2 — Descoped (manifests not used at runtime)

## Test

- [x] `docker build --target openclaw .` succeeds with no `curl | bash`
- [x] All runtime version ARGs are pinned (no `latest`, no floating refs)
- [x] `entrypoint.sh` has no `nullclaw` references
- [x] No `sudo` or `apt-get install` in entrypoint or startup path
- [x] `yq --version`, `file --version`, `gzip --version` available in the image
- [x] No sandbox/bubblewrap in image
- [x] No `tools.json` generated at startup
- [x] `HEALTHCHECK` uses correct port for openclaw (18789) and zeroclaw (42617)
- [x] Build-time smoke test passes for openclaw and zeroclaw
- [x] `cargo test -p clawden-core --quiet && cargo test -p clawden-cli --quiet` pass

## Follow-up

- [x] Remove `docker/tools/` directory (old manifests/scripts no longer referenced)
- [x] Add `yq` to complete `core-utils` parity without reintroducing the old tool layer

## Notes

- Audit performed 2026-03-06. Full finding list: 1 critical, 6 high, 14 medium, 9 low across security, consistency, correctness, performance, UX, and maintenance.
- Spec 052 (in-progress) handles the binary compatibility subset (glibc mismatch, arch errors).
- Unstable runtimes (PicoClaw, NanoClaw, OpenFang, IronClaw, NullClaw, MicroClaw) are not included in the Docker image until they stabilize.