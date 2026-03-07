# clawden

## Overview

ClawDen is the **developer experience layer for the xxxclaw ecosystem**. It provides a unified CLI and dashboard to run, manage, and monitor heterogeneous runtimes (OpenClaw, ZeroClaw, PicoClaw, NanoClaw, IronClaw, NullClaw, and community variants), while also acting as a runtime manager and SDK platform.

## Skills

This project uses the Agent Skills framework for domain-specific guidance.

### leanspec-sdd - Spec-Driven Development

- **Location**: See your skills directory (installed via `lean-spec skill install`, e.g., `.github/skills/leanspec-sdd/SKILL.md`)
- **Use when**: Working with specs, planning features, multi-step changes
- **Key principle**: Run `board` or `search` before creating specs

Read the skill file for complete SDD workflow guidance.

### runtime-research - Upstream Runtime Research

- **Location**: `.github/skills/runtime-research/SKILL.md`
- **Use when**: Adding/updating runtime adapters, researching upstream runtime metadata, auditing alignment with upstream repos
- **Key principle**: Research upstream via DeepWiki MCP tools before editing ClawDen runtime code

### clawden-development - Repository Coding Quality

- **Location**: `.github/skills/clawden-development/SKILL.md`
- **Use when**: Modifying ClawDen code, reviewing changes, adding tests, refactoring commands, or touching Rust/TypeScript codepaths outside adapter-specific work
- **Key principle**: Fix root causes, keep diffs focused, validate with the smallest sufficient test set, and preserve repository conventions

## Architecture

ClawDen uses a **Rust backend + React frontend** architecture:

- **Backend** (`crates/`): Cargo workspace — Axum server, clap CLI, adapter trait objects
- **Dashboard** (`dashboard/`): React 19 + Vite — consumes REST + WebSocket API
- **Skill SDK** (`sdk/`): TypeScript `@clawden/sdk` — for skill authors (most skills are TS/JS)

All communication with claw runtimes goes through the **Claw Runtime Interface** (`crates/clawden-core`) — Rust traits where each runtime has a pluggable adapter.

### Runtime Descriptor Pattern

Per-runtime metadata (install source, config format, health port, CLI args, cost tier) is
consolidated in `RuntimeDescriptor` structs in `crates/clawden-core/src/runtime_descriptor.rs`.
Subsystems (`install.rs`, `process.rs`, `config_gen.rs`, `manager.rs`) all consume descriptors
instead of per-runtime match statements. **Adding a new runtime's metadata is a single-entry
addition to the `DESCRIPTORS` array** — no other Rust files need per-runtime edits.

Full lifecycle adapters (`ClawAdapter` trait) live separately in `crates/clawden-adapters/` and
are optional — a runtime can exist as descriptor-only until a full adapter is implemented.

## Project-Specific Rules

- Rust for all backend code, strict clippy, rustfmt enforced
- React + TypeScript for dashboard
- TypeScript for Skill SDK (`@clawden/sdk`)
- Cargo workspace for Rust crates, pnpm for dashboard/SDK
- Adapters live in `crates/clawden-adapters/` (feature-gated)
- Specs follow LeanSpec SDD workflow
- All lifecycle events must be audit-logged
- Secrets are never stored in plain text
