---
status: in-progress
created: 2026-02-26
priority: critical
tags:
- umbrella
- pivot
- orchestration
created_at: 2026-02-26T02:06:55.408050677Z
updated_at: 2026-02-26T02:06:55.408050677Z
---

# ClawLab: xxxClaw Orchestration Platform

## Overview

The xxxclaw ecosystem (OpenClaw, ZeroClaw, PicoClaw, NanoClaw, IronClaw, NullClaw, MicroClaw, MimiClaw) is thriving but fragmented. Each product has its own deployment model, configuration format, monitoring approach, and skill/plugin system. **OpenClaw Mission Control** covers only OpenClaw.

ClawLab becomes the **unified orchestration platform** — the Kubernetes of claw agents. It provides a single control plane to deploy, manage, monitor, and coordinate heterogeneous claw agents across any infrastructure.

### Three Pillars

1. **Control Plane** — Unified lifecycle management with runtime adapters
2. **Fleet Orchestration** — Discovery, routing, and multi-agent coordination
3. **Developer Platform** — Cross-claw SDK, CLI, skill marketplace

## Design

### Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                    ClawLab Dashboard (Web UI)                 │
├──────────────────────────────────────────────────────────────┤
│                     REST + WebSocket API                      │
├──────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌───────────────┐  ┌──────────────────┐  │
│  │ Control Plane│  │Fleet Orchestr.│  │  Developer SDK   │  │
│  │  - Lifecycle │  │  - Discovery  │  │  - Skill Builder │  │
│  │  - Health    │  │  - Routing    │  │  - Test Harness  │  │
│  │  - Config    │  │  - Swarms     │  │  - Marketplace   │  │
│  └──────┬───────┘  └───────┬───────┘  └──────────────────┘  │
│         │                  │                                  │
│  ┌──────┴──────────────────┴─────────────────────────────┐   │
│  │           Claw Runtime Interface (CRI)                │   │
│  │  Adapter pattern — pluggable drivers per runtime      │   │
│  └──┬──────┬──────┬──────┬──────┬──────┬──────┬──────┬───┘   │
│     │      │      │      │      │      │      │      │       │
│    OC     ZC     PC     NC     IC     NuC   MiC    MiM      │
│  (Open) (Zero) (Pico) (Nano) (Iron) (Null)(Micro)(Mimi)     │
└──────────────────────────────────────────────────────────────┘
```

### Tech Stack
- **Backend**: Rust (Axum for HTTP/WS, tokio async runtime, SQLx for DB)
- **CLI**: Rust (clap) — ships as same binary (`clawlab` subcommands)
- **Dashboard**: React 19 + Tailwind + shadcn/ui + Vite
- **Database**: SQLite (embedded) → PostgreSQL (production)
- **Communication**: WebSocket for real-time, HTTP for control
- **Adapters**: Rust trait objects — native for Rust runtimes, subprocess/HTTP for others
- **Skill SDK**: TypeScript `@clawlab/sdk` (most skill authors use TS/JS)

## Plan

- [ ] Claw Runtime Interface / Adapter Layer (010)
- [ ] Control Plane & Agent Lifecycle (011)
- [ ] Fleet Discovery & Task Routing (012)
- [ ] Unified Configuration Management (013)
- [ ] Web Dashboard (014)
- [ ] Cross-Claw Developer SDK & CLI (015)
- [ ] Project Setup & Scaffolding (016)

## Test

- [ ] Can register and manage at least 2 different claw runtimes
- [ ] Health checks detect agent failures and report status
- [ ] Task routing sends work to appropriate agent type
- [ ] Dashboard displays real-time fleet status
- [ ] SDK can build a skill that works across multiple runtimes

## Notes

- Pivot from browser-use demo engine (specs 001-008 archived)
- The original "browser use" capability could become a ClawLab skill
- Inspiration: Kubernetes CRI, Docker Compose, Terraform providers
- Community can contribute adapters for new claw variants
- Rust chosen for ecosystem alignment (ZeroClaw, IronClaw, MicroClaw are Rust) and single-binary deployment
