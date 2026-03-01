---
status: planned
created: 2026-03-01
priority: high
tags:
- architecture
- refactor
- cli
- core
depends_on:
- 011-control-plane
- 010-claw-runtime-interface
parent: 009-orchestration-platform
created_at: 2026-03-01T06:18:51.745164Z
updated_at: 2026-03-01T06:18:51.745164Z
---

# CLI-Direct Architecture — Eliminate Mandatory Server Dependency

## Overview

Refactor ClawDen so the CLI operates directly against `clawden-core` instead of requiring a running `clawden-server` HTTP API for every operation. The server becomes optional — needed only when the dashboard UI is desired.

### Problem

Today, every CLI command (`clawden run`, `up`, `ps`, `stop`, `channels`) is a thin HTTP client that sends requests to `clawden-server` at `http://127.0.0.1:8080`. This means users must:

1. Start the server process first
2. Then use the CLI to do anything

This is unnecessarily complex for the primary use case: running a claw runtime on your machine. The server+CLI split makes sense for Kubernetes-style remote fleet management, but ClawDen's target audience includes hobbyists, students, and solo developers who just want `clawden run zeroclaw --channel telegram` to work.

The direct-install spec (022) partially addresses this by adding a CLI-local path for `--no-docker` mode, but it still assumes the server is the primary orchestration path for Docker mode — creating two divergent code paths.

### Goal

`clawden run zeroclaw --channel telegram` works immediately without starting a separate server process. The server is only started explicitly when the user wants the web dashboard.

## Design

### Architecture Change

**Before** (current):
```
User → CLI (reqwest HTTP) → Server (Axum) → LifecycleManager → Adapters
```

**After** (proposed):
```
User → CLI → clawden-core (direct library calls)
               ├── LifecycleManager
               ├── ProcessManager (new — PID files, logs, process spawning)
               ├── AuditLog
               ├── AdapterRegistry
               └── ConfigManager

User → clawden dashboard → Server (thin Axum wrapper over clawden-core)
                             → Dashboard (React UI)
```

### What moves into `clawden-core`

| Component            | Currently in                  | Moves to       |
| -------------------- | ----------------------------- | -------------- |
| `LifecycleManager`   | `clawden-server/manager.rs`   | `clawden-core` |
| `AgentState` machine | `clawden-server/lifecycle.rs` | `clawden-core` |
| `AuditLog`           | `clawden-server/audit.rs`     | `clawden-core` |
| `ChannelStore`       | `clawden-server/channels.rs`  | `clawden-core` |
| `SwarmCoordinator`   | `clawden-server/swarm.rs`     | `clawden-core` |
| `DiscoveryService`   | `clawden-server/discovery.rs` | `clawden-core` |

What stays in `clawden-server`: Axum router, HTTP handlers (thin wrappers), WebSocket streaming, static file serving, and server bootstrap APIs invoked by the `clawden dashboard` CLI command.

### ProcessManager (new in `clawden-core`)

Unified process management for both Docker and direct modes:

```rust
pub enum ExecutionMode { Docker, Direct, Auto }

pub struct ProcessManager {
    mode: ExecutionMode,
    state_dir: PathBuf,   // ~/.clawden/run/
    log_dir: PathBuf,     // ~/.clawden/logs/
}
```

Handles: process spawning (Docker containers or native), PID files, log files, health polling, graceful shutdown, crash restart with backoff.

### CLI Changes

- Remove `reqwest` dependency — CLI calls `clawden-core` directly
- Add `clawden dashboard` subcommand in `clawden-cli` (starts Axum server + opens browser)
- All existing commands (`run`, `up`, `ps`, `stop`, `channels`) work without server

### Server as Optional Dashboard Host

```bash
# Primary workflow — no server needed
clawden run zeroclaw --channel telegram
clawden up && clawden ps && clawden stop

# When you want the web UI
clawden dashboard              # starts server on :8080, opens browser
clawden dashboard --port 3000  # custom port
```

## Plan

- [ ] Move `LifecycleManager`, `AgentState`, `AuditLog` from `clawden-server` to `clawden-core`
- [ ] Move `ChannelStore`, `SwarmCoordinator`, `DiscoveryService` to `clawden-core`
- [ ] Create `ProcessManager` in `clawden-core` (Docker + Direct modes)
- [ ] Refactor `clawden-server` to thin HTTP wrapper over `clawden-core`
- [ ] Rewrite `clawden-cli` to call `clawden-core` directly (remove `reqwest`)
- [ ] Add `clawden dashboard` subcommand
- [ ] Verify existing dashboard API endpoints still work

## Test

- [ ] `clawden run zeroclaw` works without server running
- [ ] `clawden ps` / `clawden stop` work without server running
- [ ] `clawden dashboard` starts server and dashboard is accessible
- [ ] All existing REST API endpoints work when server is running
- [ ] Audit log captures events from both CLI-direct and server paths