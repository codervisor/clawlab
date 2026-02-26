---
status: planned
created: 2026-02-26
priority: critical
tags:
- infra
- setup
- monorepo
created_at: 2026-02-26T02:08:29.576100007Z
updated_at: 2026-02-26T02:08:29.576100007Z
---

# Project Setup & Monorepo Scaffolding

## Overview

Scaffold the ClawLab project with a Cargo workspace (Rust backend + CLI) and a separate React frontend for the dashboard. The Skill SDK remains TypeScript (npm).

## Design

### Project Structure
```
clawlab/
├── Cargo.toml              # Workspace root
├── crates/
│   ├── clawlab-core/       # CRI traits, types, shared utilities
│   ├── clawlab-server/     # Axum HTTP/WS API server (control plane + fleet)
│   ├── clawlab-cli/        # CLI binary (clap) — agent, fleet, config, skill cmds
│   ├── clawlab-config/     # Config schema (serde), translators, secret vault
│   └── clawlab-adapters/   # Built-in adapters (feature-gated)
│       ├── openclaw/
│       ├── zeroclaw/
│       ├── picoclaw/
│       ├── nanoclaw/
│       ├── ironclaw/
│       └── nullclaw/
├── dashboard/              # React 19 + Vite + Tailwind + shadcn/ui
│   ├── package.json
│   └── src/
├── sdk/                    # @clawlab/sdk — TypeScript skill SDK
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

- [ ] Initialize Cargo workspace with crate structure
- [ ] Set up `clawlab-core` crate with placeholder trait
- [ ] Set up `clawlab-server` crate with Axum hello-world
- [ ] Set up `clawlab-cli` crate with clap skeleton
- [ ] Scaffold React dashboard with Vite + Tailwind + shadcn/ui
- [ ] Scaffold TypeScript SDK with tsup + Vitest
- [ ] Configure GitHub Actions CI (cargo test + cargo clippy + pnpm test)
- [ ] Add Makefile/justfile for common dev commands

## Test

- [ ] `cargo build` compiles all crates
- [ ] `cargo test` passes for all crates
- [ ] `cargo clippy` reports no warnings
- [ ] Dashboard `pnpm dev` starts dev server
- [ ] CI pipeline passes on clean checkout
