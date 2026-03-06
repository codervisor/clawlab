---
status: complete
created: 2026-03-06
priority: medium
tags:
- docker
- deployment
- registry
- ux
parent: 017-docker-runtime-images
created_at: 2026-03-06T09:14:26.713701546Z
updated_at: 2026-03-06T14:34:06.612738Z
completed_at: 2026-03-06T14:34:06.612738Z
transitions:
- status: in-progress
  at: 2026-03-06T14:25:57.249997Z
- status: complete
  at: 2026-03-06T14:34:06.612738Z
---

# Runtime Image Repository & Tagging Simplification

## Overview

The current Docker publishing scheme exposes runtime selection through tags on a single repository, for example `ghcr.io/codervisor/clawden:openclaw` and `ghcr.io/codervisor/clawden:zeroclaw-browser`. That creates two UX problems:

1. Repository identity is generic (`clawden`) while the actual artifact a user wants is runtime-specific.
2. The current workflow logic mixes ClawDen release tags with runtime selector tags, which leads to awkward composed tags and makes version semantics unclear.

We want to simplify the public container surface so each runtime publishes to its own GHCR repository under the organization namespace, with immutable tags derived from the runtime version and an optional capability suffix.

## Design

Adopt per-runtime repositories in GHCR:

- `ghcr.io/codervisor/openclaw`
- `ghcr.io/codervisor/zeroclaw`

Adopt runtime-version-based tags:

- Base image: `<runtime-version>`
- Browser variant: `<runtime-version>-browser`
- Computer variant: `<runtime-version>-computer`

Examples:

- `ghcr.io/codervisor/openclaw:2026.3.2`
- `ghcr.io/codervisor/openclaw:2026.3.2-browser`
- `ghcr.io/codervisor/openclaw:2026.3.2-computer`
- `ghcr.io/codervisor/zeroclaw:0.1.7`
- `ghcr.io/codervisor/zeroclaw:0.1.7-browser`
- `ghcr.io/codervisor/zeroclaw:0.1.7-computer`

The workflow should derive published tags from the Dockerfile-pinned runtime versions rather than the ClawDen Git tag. The workflow_dispatch surface should select which runtime/variant to publish, not accept an independent image-tag input.

Scope of change:

- Update `.github/workflows/docker.yml` to publish to per-runtime repositories.
- Derive immutable tags from `OPENCLAW_VERSION` and `ZEROCLAW_VERSION` in `docker/Dockerfile`.
- Decide whether to keep moving aliases such as `latest`, `browser`, and `computer` per runtime; if retained, define them explicitly and keep them secondary to immutable tags.
- Update docs, Compose examples, CLI defaults, and tests that currently refer to `ghcr.io/codervisor/clawden:*`.

Non-goals:

- Reworking Docker build targets or runtime contents.
- Changing runtime installation logic inside the image.
- Introducing cross-runtime shared public image names.

## Plan

- [x] Define the exact public naming contract for per-runtime repositories and variant suffixes.
- [x] Update the Docker publish workflow to map each matrix target to a repository name and runtime-version-derived tags.
- [x] Keep or remove moving aliases intentionally and document the decision.
- [x] Update user-facing docs and Compose examples to reference the new image names.
- [x] Update CLI defaults and tests that assume `ghcr.io/codervisor/clawden:openclaw`.
- [x] Verify the workflow emits the expected tags for release and manual dispatch paths.

## Test

- [x] For OpenClaw, confirm generated tags include `ghcr.io/codervisor/openclaw:<version>`, `:<version>-browser`, and `:<version>-computer` as applicable.
- [x] For ZeroClaw, confirm generated tags include `ghcr.io/codervisor/zeroclaw:<version>`, `:<version>-browser`, and `:<version>-computer` as applicable.
- [x] Confirm manual dispatch selects runtime/variant without requiring a separate freeform image-tag input.
- [x] Confirm docs and CLI defaults point at the new repository names.
- [x] Confirm there are no remaining references to the old single-repository image names outside intentional migration notes.

## Notes

This is primarily a packaging and distribution UX change. The key design choice is whether moving aliases remain part of the public contract. Immutable runtime-version tags should be the source of truth either way.

- Moving aliases are retained as secondary convenience tags per runtime repository: `latest` for the base image, plus `browser` and `computer` for the capability variants.
- Non-release publishes keep using preview aliases (`preview`, `preview-browser`, `preview-computer`) so the immutable runtime-version tags remain release-only.