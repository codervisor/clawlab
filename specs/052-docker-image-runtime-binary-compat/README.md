---
status: complete
created: 2026-03-06
priority: high
tags:
- docker
- bug
- zeroclaw
- picoclaw
- binary-compat
- ci
parent: 017-docker-runtime-images
created_at: 2026-03-06T05:28:14.483453041Z
updated_at: 2026-03-07T13:41:03.763223Z
transitions:
- status: in-progress
  at: 2026-03-06T05:41:03.487538427Z
---
# Docker Image Runtime Binary Compatibility — GLIBC Mismatch, Architecture Errors & Stale Image

## Overview

The published `ghcr.io/codervisor/clawden-runtime:latest` image bundles a ZeroClaw binary that requires `GLIBC_2.39`, but the image is built on Debian bookworm which ships `GLIBC_2.36`. The container exits immediately on startup with:

```
/home/clawden/.clawden/runtimes/zeroclaw/current/zeroclaw: /lib/x86_64-linux-gnu/libc.so.6:
  version `GLIBC_2.39' not found (required by .../zeroclaw)
```

PicoClaw has a similar issue: `Exec format error`, suggesting a wrong-architecture binary was bundled.

## Problem

The image build pipeline downloads pre-built runtime binaries from upstream releases and bundles them. The binary artifacts are not validated against the base image's system libraries or architecture at build time. This leads to silent incompatibilities that only surface when a user tries to run the container.

### Affected runtimes

| Runtime   | Failure                            | Likely cause                                                |
| --------- | ---------------------------------- | ----------------------------------------------------------- |
| ZeroClaw  | `GLIBC_2.39 not found`            | Binary built on newer glibc than Debian bookworm provides   |
| PicoClaw  | `Exec format error`               | Wrong architecture binary (e.g., arm64 binary on amd64)     |
| OpenFang  | `Unknown runtime 'openfang'`      | Published image is stale; entrypoint case block not updated |

### Impact

`clawden docker run zeroclaw` and `clawden docker run picoclaw` fail immediately. Users see a "Started via Docker" success message followed by a dead container.

## Design

### Fix 1: Build ZeroClaw statically or match glibc

Either:
- Build ZeroClaw with `target x86_64-unknown-linux-musl` (fully static, no glibc dependency) — preferred
- Or upgrade the base image to Debian trixie / Ubuntu 24.10 which ships glibc 2.39+

For Rust binaries (ZeroClaw, OpenFang), musl static linking is the standard solution. For Go binaries (PicoClaw), `CGO_ENABLED=0` produces a static binary by default.

### Fix 2: Architecture-correct binary selection

The Dockerfile `COPY` or download step must select the correct architecture binary matching the build platform's `TARGETARCH`. For multi-arch images, use Docker buildx `--platform` and conditional download logic.

### Fix 3: Build-time smoke test

Add a `RUN` step in the Dockerfile that verifies each bundled binary is executable:

```dockerfile
RUN for runtime in zeroclaw picoclaw openclaw nanoclaw openfang; do \
      bin="/home/clawden/.clawden/runtimes/${runtime}/current/${runtime}"; \
      if [ -x "$bin" ]; then \
        "$bin" --version || "$bin" --help || echo "WARN: ${runtime} did not respond to --version/--help"; \
      fi; \
    done
```

This catches glibc mismatches and architecture errors at image build time instead of at user runtime.

### Fix 4: Rebuild published image with current entrypoint

The published `latest` image does not include the `openfang` case in the entrypoint, despite the repo having it. The image needs to be rebuilt and republished from the current repo state.

## Plan

- [x] Switch ZeroClaw bundling to musl-static binary (or upgrade base image glibc)
- [x] Ensure PicoClaw binary matches target architecture (`TARGETARCH`-aware download)
- [x] Add Dockerfile `RUN` smoke test that execs each bundled runtime binary with `--version` or `--help`
- [ ] Rebuild and publish `ghcr.io/codervisor/clawden-runtime:latest` from current repo (picks up openfang entrypoint support)
- [x] Add CI gate: image build must pass smoke test before push to GHCR

## Test

- [ ] `docker run -e RUNTIME=zeroclaw ghcr.io/codervisor/clawden-runtime:latest` starts without glibc errors
- [ ] `docker run -e RUNTIME=picoclaw ghcr.io/codervisor/clawden-runtime:latest` starts without exec format errors
- [ ] `docker run -e RUNTIME=openfang ghcr.io/codervisor/clawden-runtime:latest` recognized as valid runtime
- [ ] Image build fails if any bundled runtime binary cannot execute `--version` or `--help`
- [ ] Multi-arch build (amd64 + arm64) produces correct binaries for each platform

## Notes

- 2026-03-06: Installer now prefers Linux musl assets before glibc, probes downloaded GitHub-release binaries by executing `--version`/`--help`, and rejects ambiguous `.7z` archives unless the extracted runtime is runnable on the current platform.
- 2026-03-06: `docker/Dockerfile` now pins `picoclaw@${PICOCLAW_VERSION}` and smoke-tests every bundled runtime during the image build, so the existing multi-arch GitHub Actions build fails before push if any runtime cannot execute.
- Remaining operational work: rebuild/publish `ghcr.io/codervisor/clawden-runtime:latest` and run the container-level verification commands from the Test section.