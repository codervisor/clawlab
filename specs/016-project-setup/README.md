---
status: in-progress
created: 2026-02-26
priority: critical
tags:
- infra
- setup
- monorepo
parent: 009-orchestration-platform
created_at: 2026-02-26T02:08:29.576100007Z
updated_at: 2026-02-26T03:07:30.691898682Z
transitions:
- status: in-progress
  at: 2026-02-26T03:07:30.691898682Z
---
# Project Setup & Monorepo Scaffolding

## Overview

Scaffold the ClawDen project with a Cargo workspace (Rust backend + CLI) and a separate React frontend for the dashboard. The Skill SDK remains TypeScript (npm).

## Design

### Project Structure
```
clawden/
├── Cargo.toml              # Workspace root
├── crates/
│   ├── clawden-core/       # CRI traits, types, shared utilities
│   ├── clawden-server/     # Axum HTTP/WS API server (control plane + fleet)
│   ├── clawden-cli/        # CLI binary (clap) — agent, fleet, config, skill cmds
│   ├── clawden-config/     # Config schema (serde), translators, secret vault
│   └── clawden-adapters/   # Built-in adapters (feature-gated)
│       ├── openclaw/
│       ├── zeroclaw/
│       ├── picoclaw/
│       ├── nanoclaw/
│       ├── ironclaw/
│       └── nullclaw/
├── dashboard/              # React 19 + Vite + Tailwind + shadcn/ui
│   ├── package.json
│   └── src/
├── sdk/                    # @clawden/sdk — TypeScript skill SDK
│   ├── package.json
│   └── src/
├── specs/                  # LeanSpec specs
├── .github/                # CI/CD
└── README.md
```

### Tooling
- **Rust**: Cargo workspace, rustfmt, clippy
- **Build**: `cargo build --release` → single binary
- **Test**: `cargo test` (Rust) + Vitest (dashboard/SDK)
- **Frontend**: pnpm (dashboard + SDK only)
- **CI**: GitHub Actions (Rust + Node.js matrix)

## Plan

- [x] Initialize Cargo workspace with crate structure
- [x] Set up `clawden-core` crate with placeholder trait
- [x] Set up `clawden-server` crate with Axum hello-world
- [x] Set up `clawden-cli` crate with clap skeleton
- [ ] Scaffold React dashboard with Vite + Tailwind + shadcn/ui
- [x] Scaffold TypeScript SDK with tsup + Vitest
- [x] Configure GitHub Actions CI (cargo test + cargo clippy + pnpm test)
- [x] Add Makefile/justfile for common dev commands

## Test

- [x] `cargo build` compiles all crates
- [x] `cargo test` passes for all crates
- [x] `cargo clippy` reports no warnings
- [x] Dashboard `pnpm dev` starts dev server
- [ ] CI pipeline passes on clean checkout