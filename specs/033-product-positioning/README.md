---
status: planned
created: 2026-03-03
priority: high
tags:
- positioning
- product
- ux
- strategy
created_at: 2026-03-03T08:49:22.936640Z
updated_at: 2026-03-03T08:49:22.936640Z
---

# ClawDen Product Positioning — UX Shell, Runtime Manager, SDK Platform

## Overview

ClawDen has evolved beyond "orchestration platform" into three distinct, complementary product roles. This spec clarifies ClawDen's identity and establishes positioning language to guide architecture decisions, documentation, and marketing.

### Problem

The current positioning — "unified orchestration platform" / "Kubernetes of claw agents" — is technically accurate but creates two issues:

1. **Over-indexes on infra.** It frames ClawDen as ops tooling for fleet management, when most users are solo developers or hobbyists running 1–2 runtimes locally. The CLI-Direct architecture (023) already acknowledged this by eliminating the mandatory server.
2. **Under-sells the UX/DX value.** ClawDen's biggest value isn't orchestration — it's that a user can `npm i -g clawden && clawden run zeroclaw --channel telegram` without understanding Docker, config formats, or runtime internals.

### The Three Roles

#### 1. UX Shell (primary)

ClawDen is the **unified command-line and dashboard experience** for the xxxclaw ecosystem. Like how `gh` wraps Git+GitHub into a cohesive workflow, ClawDen wraps heterogeneous claw runtimes behind a single, opinionated interface.

**Analogy:** `gh` CLI / Homebrew / Docker Desktop

Key UX surfaces:
- CLI commands: `run`, `up`, `ps`, `stop`, `channels`, `config`
- Guided onboarding: `clawden init` → interactive runtime selection
- Dashboard: real-time monitoring, log streaming, channel management
- Config generation: `clawden config gen` → unified TOML regardless of runtime

What this means for decisions:
- CLI ergonomics and error messages are first-class concerns
- Default behaviors should "just work" for the single-runtime case
- Power-user features (fleet, swarm) are discoverable but not required

#### 2. Runtime Manager (secondary)

ClawDen manages claw runtime **installations, versions, and updates** — exactly like `nvm` manages Node.js versions or `rustup` manages Rust toolchains.

**Analogy:** nvm / rustup / pyenv

Key capabilities:
- `clawden pull zeroclaw` — download/install a runtime
- `clawden pull zeroclaw@0.5.2` — pin a specific version
- `clawden update` — check for and apply runtime updates (spec 028)
- Runtime catalog — knows all available runtimes and their install methods
- Channel management — Docker images vs. direct binaries vs. source builds

What this means for decisions:
- ClawDen must maintain a runtime registry/catalog (currently `RuntimeCatalog`)
- Version resolution and caching are real product features, not implementation details
- Offline support matters (pre-pulled runtimes should work without network)

#### 3. SDK Platform (tertiary)

ClawDen provides the **cross-runtime development kit** for building skills/plugins that work across claw variants.

**Analogy:** Terraform Provider SDK / VS Code Extension API

Key capabilities:
- `@clawden/sdk` — TypeScript SDK with `defineSkill()` API
- `clawden skill create` / `clawden skill test` — scaffolding and cross-runtime testing
- Adapter abstraction — skills don't know which runtime they're running on
- (Future) Skill marketplace

What this means for decisions:
- SDK API stability is critical — breaking changes hurt ecosystem
- Cross-runtime compatibility testing is a product feature
- Skill authors are a distinct persona from runtime users

### Positioning Statement

> **ClawDen** is the developer experience layer for the xxxclaw ecosystem. Install any claw runtime in one command, manage versions and updates automatically, and build skills that work everywhere — all through a single CLI and dashboard.

### Elevator Pitches by Role

| Role | One-liner |
|------|-----------|
| UX Shell | "One CLI to run any claw agent — no config files, no Docker knowledge required" |
| Runtime Manager | "nvm for claw runtimes — install, switch, and update with one command" |
| SDK Platform | "Build once, run on any claw — cross-runtime skills with TypeScript" |

## Design

### Persona Alignment

| Persona | Primary role used | Entry point |
|---------|-------------------|-------------|
| Hobbyist/student | UX Shell | `npm i -g clawden && clawden run zeroclaw` |
| Solo developer | UX Shell + Runtime Manager | `clawden pull openclaw && clawden run openclaw --channel telegram` |
| Skill author | SDK Platform | `clawden skill create my-skill` |
| Team/enterprise | All three + fleet features | `clawden dashboard` + fleet orchestration |

### Impact on Architecture

This positioning reinforces several existing architectural decisions:
- **CLI-Direct (023)**: Correct — UX Shell should work without server overhead
- **Guided onboarding (026)**: Correct — first-run experience is critical for UX Shell role
- **Runtime pull/update (028)**: Correct — this is core Runtime Manager functionality
- **SDK package (015, 019)**: Correct — SDK is a distinct distribution concern

Potential gaps this positioning reveals:
- **Runtime version pinning**: `clawden pull zeroclaw@0.5.2` not yet implemented
- **Offline catalog**: Pre-pulled runtimes should work without network access
- **Persona-aware docs**: README and docs should speak to the persona, not the architecture
- **`clawden doctor`**: A diagnostic command to verify runtime health, versions, and config — common in UX-first tools

### Documentation & Messaging Guidance

- README should lead with the UX Shell pitch, not architecture diagrams
- `--help` text should use plain language ("Run a claw agent" not "Invoke lifecycle management")
- Error messages should suggest next steps, not expose internal state
- Landing page structure: "Get started in 30 seconds" → "Manage your runtimes" → "Build skills"

## Plan

- [ ] Update README.md to reflect UX Shell-first positioning
- [ ] Audit CLI `--help` text for plain-language clarity
- [ ] Add `clawden doctor` diagnostic command
- [ ] Implement runtime version pinning (`@version` syntax)
- [ ] Write persona-aligned documentation sections
- [ ] Review AGENTS.md description to align with new positioning

## Test

- [ ] README communicates value proposition in first 3 lines
- [ ] `clawden --help` output is understandable by someone who has never seen ClawDen
- [ ] Each persona can complete their entry-point workflow in under 60 seconds
- [ ] Positioning language is consistent across CLI, dashboard, docs, and package descriptions