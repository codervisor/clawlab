# clawlab

## Overview

ClawLab is the **unified orchestration platform for the xxxclaw ecosystem**. It provides a single control plane to deploy, manage, monitor, and coordinate heterogeneous AI agent runtimes — OpenClaw, ZeroClaw, PicoClaw, NanoClaw, IronClaw, NullClaw, and community variants.

## Skills

This project uses the Agent Skills framework for domain-specific guidance.

### leanspec-sdd - Spec-Driven Development

- **Location**: See your skills directory (installed via `lean-spec skill install`, e.g., `.github/skills/leanspec-sdd/SKILL.md`)
- **Use when**: Working with specs, planning features, multi-step changes
- **Key principle**: Run `board` or `search` before creating specs

Read the skill file for complete SDD workflow guidance.

## Architecture

ClawLab uses a **Rust backend + React frontend** architecture:

- **Backend** (`crates/`): Cargo workspace — Axum server, clap CLI, adapter trait objects
- **Dashboard** (`dashboard/`): React 19 + Vite — consumes REST + WebSocket API
- **Skill SDK** (`sdk/`): TypeScript `@clawlab/sdk` — for skill authors (most skills are TS/JS)

All communication with claw runtimes goes through the **Claw Runtime Interface** (`crates/clawlab-core`) — Rust traits where each runtime has a pluggable adapter.

## Project-Specific Rules

- Rust for all backend code, strict clippy, rustfmt enforced
- React + TypeScript for dashboard
- TypeScript for Skill SDK (`@clawlab/sdk`)
- Cargo workspace for Rust crates, pnpm for dashboard/SDK
- Adapters live in `crates/clawlab-adapters/` (feature-gated)
- Specs follow LeanSpec SDD workflow
- All lifecycle events must be audit-logged
- Secrets are never stored in plain text
