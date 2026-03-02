---
status: planned
created: 2026-03-02
priority: high
tags:
- cli
- ux
- overhaul
depends_on:
- 023-cli-direct-architecture
parent: 009-orchestration-platform
created_at: 2026-03-02T07:28:51.120244947Z
updated_at: 2026-03-02T07:28:51.120244947Z
---

# Docker Compose UX — CLI Command Overhaul

## Overview

Overhaul ClawDen's CLI command UX to match the docker compose mental model. Today, `clawden up` blocks with no visible feedback (no log streaming), `clawden run` silently detaches, there's no `down` command to complement `up`, and the foreground/background behavior is inconsistent.

Users expect `clawden up` to feel like `docker compose up` — streaming logs in the foreground by default, with `-d` for detached mode, and a proper `down` command to tear everything down.

### Problems with Current UX

| Command | Current Behavior | Expected (docker compose style) |
| --- | --- | --- |
| `clawden up` | Starts runtimes, prints 1 line, blocks with no output | Stream all runtime logs in foreground; `-d` to detach |
| `clawden run` | Starts 1 runtime, prints pid, returns immediately | One-off foreground run; blocks until exit or Ctrl+C |
| `clawden stop` | Stops runtimes | Keep, but add `down` as the inverse of `up` |
| `clawden logs` | Prints last N lines, no follow mode | Add `-f`/`--follow` for live tailing; multi-runtime mux |
| (missing) | — | `clawden down` — stop all + cleanup |
| (missing) | — | `clawden restart` — stop + re-start |

## Design

### Command Redesign

#### `clawden up` (foreground by default)

```
clawden up [RUNTIMES...] [-d|--detach] [--no-log-prefix] [--timeout N]
```

**Attached (default — like `docker compose up`):**
- Starts all runtimes defined in clawden.yaml
- Streams interleaved logs from all runtimes to stdout, color-coded per runtime
- Blocks until Ctrl+C, then gracefully shuts down
- Each log line prefixed: `zeroclaw  | listening on port 3000`

**Detached (`-d`):**
- Starts all runtimes in background
- Prints status table, returns immediately

#### `clawden down` (new — inverse of `up`)

```
clawden down [RUNTIMES...] [--timeout N] [--remove-orphans]
```

- Stops all runtimes started by `up` **that belong to the current project** (scoped by config-path hash stored in each PID file at start time)
- Cleans up PID files and stale state
- `--remove-orphans` — stop runtimes owned by this project but no longer listed in its clawden.yaml (never touches runtimes owned by other projects/sessions)

#### `clawden run` (one-off, foreground)

```
clawden run [--rm] [-d|--detach] [--channel CH...] [--with TOOLS] RUNTIME [ARGS...]
```

- Foreground by default (streams output directly, exits when runtime exits)
- `--detach` — run in background (current behavior)
- One-off / ad-hoc semantics, not for multi-runtime orchestration

#### `clawden logs` (follow support)

```
clawden logs [-f|--follow] [--tail N] [--timestamps] [RUNTIME...]
```

- `-f` streams live logs, multiplexed across runtimes with color prefixes
- Without RUNTIME arg: all running runtimes
- `--timestamps` prepends timestamp to each line

#### `clawden restart` (new)

```
clawden restart [RUNTIMES...] [--timeout N]
```

- Stop + start for named runtimes (or all)

#### `clawden start` (new — complement of `stop`)

```
clawden start [RUNTIMES...]
```

- Re-start previously stopped runtimes using last config

### Per-Runtime Start Commands

> **Bug (pre-existing):** The CLI currently hardcodes `"daemon"` as the first argument for all runtimes in direct mode (`up.rs`, `run.rs`). This is only correct for ZeroClaw. Other runtimes use different entry subcommands:

| Runtime | Start command (direct mode) | Notes |
| --- | --- | --- |
| ZeroClaw | `zeroclaw daemon` | Rust — `daemon` subcommand |
| PicoClaw | `picoclaw gateway` | Go — `gateway` for long-running; `agent` for one-shot |
| NanoClaw | `nanoclaw` (no subcommand) | TypeScript/Node — single process |
| OpenClaw | `openclaw` (no subcommand) | TypeScript — direct process |
| NullClaw | `nullclaw daemon` (unconfirmed) | Zig — has `src/daemon.zig` |

The Docker entrypoint (`entrypoint.sh`) sidesteps the bug by using `exec "$LAUNCHER" "$@"` (pass-through).

**Fix:** Add a `start_args` field to the runtime adapter trait / install metadata so each runtime declares its own entry subcommand. Fall back to no subcommand if unset.

### Log Streaming Architecture

`ProcessManager` gains real-time log streaming via piped stdout/stderr:

```rust
pub fn stream_logs(&self, runtimes: &[String]) -> Result<LogStream>;

pub struct LogStream {
    receiver: tokio::sync::mpsc::Receiver<LogLine>,  // bounded (capacity: 4096)
}

// Back-pressure policy: when the channel is full, the oldest unread line
// is dropped and a per-runtime `dropped_lines` counter is incremented.
// The counter is printed as a warning when the consumer catches up:
//   "zeroclaw | ⚠ 12 log lines dropped (slow consumer)"

pub struct LogLine {
    pub runtime: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub text: String,
}
```

**Color coding** — each runtime gets a distinct ANSI color (cycling: cyan, yellow, green, magenta, blue, red):

```
\x1b[36mzeroclaw  |\x1b[0m Server started on port 3000
\x1b[33mnanoclaw  |\x1b[0m Connecting to Telegram...
```

### Graceful Shutdown

On Ctrl+C (SIGINT):
1. Print "Gracefully stopping..." (like docker compose)
2. Send SIGTERM to all runtimes
3. Wait up to `--timeout` seconds (default: 10)
4. SIGKILL any remaining processes
5. Print stop confirmation per runtime

On **second** Ctrl+C during shutdown: immediate SIGKILL all.

## Plan

- [ ] Add `LogStream`/`LogLine` types and `stream_logs()` to `ProcessManager`
- [ ] Refactor process spawning to capture stdout/stderr via pipe (tee to log file + stream)
- [ ] Implement color-coded log multiplexer in `clawden-cli`
- [ ] Rewrite `clawden up` — foreground streaming default, add `-d`/`--detach`
- [ ] Implement double-Ctrl+C shutdown (graceful then forced)
- [ ] Add `clawden down` command with PID cleanup and `--remove-orphans` (project-scoped via config-path hash)
- [ ] Rewrite `clawden run` — foreground streaming default, add `--detach`
- [ ] Enhance `clawden logs` — add `-f`/`--follow`, multi-runtime mux, `--timestamps`
- [ ] Add `clawden restart` command
- [ ] Add `clawden start` command
- [ ] Add `--timeout` flag to `stop`, `down`, `restart`, `up`
- [ ] Store project ownership (config-path hash) in PID files at start time; use for `down`/orphan scoping
- [ ] Replace hardcoded `"daemon"` arg with per-runtime `start_args` from install metadata (fixes PicoClaw `gateway`, NanoClaw no-subcommand, etc.)
- [ ] Audit-log all new lifecycle events: `runtime.down`, `runtime.restart`, `runtime.start`, `runtime.force_kill`
- [ ] Update CLI help text and clap command descriptions

## Test

- [ ] `clawden up` streams runtime logs to stdout in foreground mode
- [ ] `clawden up -d` starts runtimes and returns immediately with status table
- [ ] Ctrl+C during `up` sends SIGTERM and waits for graceful shutdown
- [ ] Double Ctrl+C during shutdown triggers immediate SIGKILL
- [ ] `clawden down` stops all runtimes and cleans up PID files
- [ ] `clawden run zeroclaw` blocks and streams output until exit
- [ ] `clawden run -d zeroclaw` starts in background, returns pid
- [ ] `clawden logs -f` streams live logs from all running runtimes
- [ ] `clawden logs -f zeroclaw nanoclaw` multiplexes with color prefixes
- [ ] `clawden restart` stops then re-starts specified runtimes
- [ ] Log lines are color-coded per runtime when multiple are active
- [ ] `--timeout` is respected during shutdown
- [ ] `clawden down --remove-orphans` removes runtimes not in clawden.yaml but owned by this project
- [ ] `clawden down --remove-orphans` does NOT stop runtimes owned by other projects
- [ ] All lifecycle commands (`up`, `down`, `start`, `restart`) emit audit log entries
- [ ] Forced-kill after timeout emits `runtime.force_kill` audit entry
- [ ] Log stream drops oldest lines under back-pressure and prints dropped-line warning
- [ ] `clawden up` with PicoClaw in direct mode uses `gateway` (not `daemon`) as start subcommand
- [ ] `clawden run nanoclaw` starts with no subcommand (not `daemon`)