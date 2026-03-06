---
status: planned
created: 2026-03-06
priority: high
tags:
- docker
- ux
- runtime
- container
- onboarding
parent: 017-docker-runtime-images
created_at: 2026-03-06T05:14:56.087137077Z
updated_at: 2026-03-06T05:14:56.087137077Z
---

# Docker Runtime Direct-Run UX — Self-Describing Entrypoint & Positional Runtime Selection

## Overview

`ghcr.io/codervisor/clawden-runtime:latest` currently assumes ClawDen will inject `RUNTIME` and `TOOLS`. That is internally coherent, but hostile at the container boundary: a user can pull the public image, run it directly, and get an immediate exit with a low-context error. For a published runtime image, that is the wrong default experience.

This spec makes the image usable and self-explanatory when invoked directly with `docker run`, while preserving the existing ClawDen-managed contract.

## Design

### Problem

Current behavior optimizes only for the managed path:

- `clawden docker run zeroclaw` works because ClawDen injects `RUNTIME=zeroclaw`
- `docker run ghcr.io/codervisor/clawden-runtime:latest` exits immediately
- The failure message explains that `RUNTIME` is required, but it still forces users to reverse-engineer the image contract after a failed first run

That creates three UX problems:

1. The image looks broken when used the way container users naturally try first.
2. The image contract is environment-variable-first, which is awkward for ad-hoc Docker usage.
3. The current entrypoint has no discovery surface for supported runtimes, examples, or invocation patterns.

### Goals

- Make first contact with the public image understandable without reading source.
- Support a natural direct-run form: `docker run <image> <runtime> [args...]`.
- Keep `RUNTIME=<name>` support for ClawDen-managed execution and scripts.
- Fail with high-signal guidance when invocation is incomplete.

### Non-Goals

- Do not auto-select a default runtime.
- Do not keep the container alive just to avoid exit.
- Do not turn the image into a full management CLI.
- Do not weaken the existing `clawden docker run` path or adapter contract.

### Entrypoint UX Model

The entrypoint becomes a small wrapper with two valid invocation styles.

#### Mode A: Managed env contract

Existing behavior remains supported:

```bash
docker run -e RUNTIME=zeroclaw ghcr.io/codervisor/clawden-runtime:latest
```

ClawDen continues to use this path.

#### Mode B: Positional runtime selection

If `RUNTIME` is unset and the first positional argument is a supported runtime slug, treat that argument as the runtime and pass the remaining arguments through to the runtime launcher:

```bash
docker run ghcr.io/codervisor/clawden-runtime:latest zeroclaw
docker run ghcr.io/codervisor/clawden-runtime:latest openclaw gateway
docker run ghcr.io/codervisor/clawden-runtime:latest zeroclaw --help
```

This is the most natural Docker-native usage and removes the need for users to discover the env-var contract first.

### Help and Discovery

If neither of the two valid invocation styles is used, the image should print a concise usage block and exit with a usage error.

Example output shape:

```text
ClawDen runtime image

Usage:
  docker run ghcr.io/codervisor/clawden-runtime:latest <runtime> [runtime-args...]
  docker run -e RUNTIME=<runtime> ghcr.io/codervisor/clawden-runtime:latest [runtime-args...]

Supported runtimes:
  zeroclaw, picoclaw, openclaw, nanoclaw, openfang

Examples:
  docker run --rm ghcr.io/codervisor/clawden-runtime:latest zeroclaw
  docker run --rm -e RUNTIME=openclaw ghcr.io/codervisor/clawden-runtime:latest gateway
```

Additionally:

- `docker run <image> --help` prints wrapper help, not a low-context error
- `docker run <image> --list-runtimes` prints supported runtime slugs
- `docker run <image> <runtime> --help` passes through to the selected runtime

### Argument Resolution Rules

Resolution order should be explicit and stable:

1. If `RUNTIME` env var is set, it wins.
2. Else if first positional arg matches a supported runtime slug, use it as runtime.
3. Else if first positional arg is `--help` or `--list-runtimes`, run wrapper behavior.
4. Else print usage with examples and exit non-zero.

This keeps ClawDen-managed behavior deterministic while making raw Docker usage ergonomic.

### Supported Runtime Source of Truth

The wrapper should not duplicate runtime knowledge in multiple places without need. The supported runtime list should be maintained in one place in the entrypoint or generated from the installed runtime layout during image build, so the help output and runtime validation cannot drift.

### Documentation Updates

Public-facing docs should stop implying that the image is only intended for ClawDen-internal use.

Update docs to show three tiers clearly:

1. Preferred: `clawden docker run <runtime>`
2. Supported direct Docker usage: `docker run <image> <runtime>`
3. Advanced/scripted env contract: `docker run -e RUNTIME=<runtime> <image>`

### Tradeoff

This adds a thin wrapper concern to the image entrypoint, but that is the correct place for it. The container boundary is a product surface, not just an implementation detail. A published image should be self-describing.

## Plan

- [ ] Refactor `docker/entrypoint.sh` to accept positional runtime selection when `RUNTIME` is unset
- [ ] Add wrapper-level `--help` and `--list-runtimes`
- [ ] Centralize supported runtime slug list to avoid drift between help and launch logic
- [ ] Preserve existing `RUNTIME` + `TOOLS` env-driven behavior for ClawDen-managed runs
- [ ] Update Docker image docs and README examples to show direct Docker usage first-class
- [ ] Add regression tests for both env-driven and positional invocation modes

## Test

- [ ] `docker run ghcr.io/codervisor/clawden-runtime:latest` prints usage guidance and exits with usage error
- [ ] `docker run ghcr.io/codervisor/clawden-runtime:latest --help` prints wrapper help
- [ ] `docker run ghcr.io/codervisor/clawden-runtime:latest --list-runtimes` prints supported runtimes
- [ ] `docker run ghcr.io/codervisor/clawden-runtime:latest zeroclaw` starts ZeroClaw without requiring `-e RUNTIME=...`
- [ ] `docker run ghcr.io/codervisor/clawden-runtime:latest zeroclaw --help` passes through to ZeroClaw help output
- [ ] `docker run -e RUNTIME=zeroclaw ghcr.io/codervisor/clawden-runtime:latest` still works unchanged
- [ ] `clawden docker run zeroclaw` still injects `RUNTIME` and works unchanged
- [ ] Invalid runtime names print a usage message that includes supported runtimes

## Notes

This is a follow-up to the original runtime image design, not a reversal of it. Spec 017 correctly centralized runtime selection inside the image, but the image boundary still needs a humane first-run experience.

This also complements spec 037: `clawden docker run` remains the preferred ClawDen UX, but the raw published image should not feel broken when used directly.