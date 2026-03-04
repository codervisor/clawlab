---
status: complete
created: 2026-03-04
priority: high
tags:
- cli
- run
- ux
- passthrough
- product-positioning
created_at: 2026-03-04T12:50:52.266558Z
updated_at: 2026-03-04T12:58:56.644416Z
completed_at: 2026-03-04T12:58:56.644416Z
transitions:
- status: in-progress
  at: 2026-03-04T12:50:52.266558Z
- status: complete
  at: 2026-03-04T12:58:56.644416Z
---

# Run Clean Passthrough — Remove Implicit Subcommands from `clawden run`

## Overview

`clawden run <runtime> [args...]` currently injects implicit subcommands (`daemon`, `gateway`) before forwarding arguments to the runtime binary. This breaks the `uv run`-style transparent execution model established in spec 033. A true passthrough means ClawDen should never silently modify the user's command — the user types exactly what the runtime would accept, and ClawDen's only job is to ensure it's installed, configured, and launched.

### Problem

Today, `clawden run zeroclaw --verbose` actually executes:

```
zeroclaw daemon --config-dir /path/to/config --verbose
```

The implicit `daemon` subcommand is injected by `runtime_start_args()` in `clawden-core/src/install.rs`:

```rust
pub fn runtime_start_args(runtime: &str) -> Vec<String> {
    match runtime {
        "zeroclaw" => vec!["daemon".to_string()],
        "picoclaw" => vec!["gateway".to_string()],
        "openfang" => vec!["daemon".to_string()],
        "nullclaw" => vec!["daemon".to_string()],
        _ => Vec::new(),
    }
}
```

This creates several issues:

1. **Not transparent.** The user doesn't know `daemon` was injected. When they read zeroclaw's own --help, they see subcommands like `daemon`, `chat`, `repl`, `serve` — but `clawden run` silently picks one for them. This is the opposite of `uv run`, where `uv run ruff check .` literally runs `ruff check .`.

2. **Locks users into one mode.** Want to run `zeroclaw repl` or `zeroclaw chat --once`? Can't — ClawDen always prepends `daemon`. The user must bypass ClawDen entirely or hack around it with `--`.

3. **Fragile coupling.** If zeroclaw renames `daemon` to `serve` in v0.6, ClawDen breaks. The hardcoded mapping turns ClawDen into a source of version-specific breakage rather than an abstraction.

4. **Inconsistent behavior across runtimes.** OpenClaw and NanoClaw get clean passthrough (empty `start_args`), while ZeroClaw/PicoClaw/NullClaw get implicit subcommands. Users switching between runtimes can't form a mental model of what `clawden run` actually does.

5. **Violates the "UX shell" contract.** Spec 033 says ClawDen is like `gh` or `uv` — a UX layer that doesn't modify semantics. Injecting subcommands modifies semantics.

### Goal

Make `clawden run <runtime> [args...]` a **clean passthrough**: ClawDen handles installation, config injection (via env vars and `--config-dir`), and lifecycle — but never inserts arguments the user didn't type.

## Design

### Core Change: Remove `runtime_start_args()`

`runtime_start_args()` is deleted. The `InstalledRuntime.start_args` field is removed (or made always-empty). `run.rs` builds the argument list purely from:

1. Config injection args (`--config-dir <path>` when config translation applies)
2. User-provided `extra_args`

That's it. No implicit subcommands.

### What Changes for Users

| Before | After | Notes |
|--------|-------|-------|
| `clawden run zeroclaw` | `clawden run zeroclaw daemon` | User must type `daemon` explicitly |
| `clawden run zeroclaw --verbose` | `clawden run zeroclaw daemon --verbose` | Same |
| `clawden run picoclaw` | `clawden run picoclaw gateway` | User must type `gateway` explicitly |
| `clawden run zeroclaw repl` | `clawden run zeroclaw repl` | Now works! (was impossible before) |
| `clawden run zeroclaw chat --once` | `clawden run zeroclaw chat --once` | Now works! |
| `clawden run openclaw` | `clawden run openclaw` | No change (was already passthrough) |

### Smart Default Hint

When the user runs `clawden run zeroclaw` with no subcommand and the runtime exits with a non-zero code (likely its own "missing subcommand" error), ClawDen shows a helpful hint:

```
Hint: zeroclaw expects a subcommand. Common options:
  clawden run zeroclaw daemon     — run as background daemon
  clawden run zeroclaw repl       — interactive REPL
  clawden run zeroclaw chat       — single-turn chat
Run `clawden run zeroclaw --help` to see all subcommands.
```

This preserves discoverability without injecting behavior.

### Runtime Subcommand Hints Registry

Replace `runtime_start_args()` with a non-injected hint registry:

```rust
/// Common subcommands for known runtimes, used for hint messages only.
/// Never injected into the command line.
pub fn runtime_subcommand_hints(runtime: &str) -> &'static [(&'static str, &'static str)] {
    match runtime {
        "zeroclaw" => &[
            ("daemon", "run as background daemon"),
            ("repl", "interactive REPL"),
            ("chat", "single-turn chat"),
            ("serve", "HTTP API server"),
        ],
        "picoclaw" => &[
            ("gateway", "HTTP gateway mode"),
            ("proxy", "reverse proxy mode"),
        ],
        "openfang" => &[
            ("daemon", "run as background daemon"),
            ("serve", "HTTP API server"),
        ],
        "nullclaw" => &[
            ("daemon", "run as background daemon"),
        ],
        _ => &[],
    }
}
```

### Config-Dir Injection Stays

`--config-dir` injection is **not** affected by this change. Config translation (generating runtime-native config files from `clawden.yaml`) is ClawDen's core value — it's what makes ClawDen a UX shell, not a dumb exec wrapper. The `--config-dir` arg is ClawDen infrastructure, not a user-facing subcommand.

However, `--config-dir` should be injected **intelligently**:
- Only when a `clawden.yaml` exists or CLI flags require config generation
- Placed after the runtime's subcommand in the arg list (e.g., `zeroclaw daemon --config-dir /path`, not `zeroclaw --config-dir /path daemon`)

### `runtime_supported_extra_args()` Update

The validation allowlist also needs updating. Currently it only allows `--config-dir`, `--port`, `--host`. In a clean passthrough model, ClawDen should **not validate runtime args at all** — the runtime binary is the authority on what flags it accepts.

Remove `runtime_supported_extra_args()` and `validate_runtime_args()`. The warning about unsupported flags is noise in a passthrough model — if the user typed it, they meant it.

### Impact on `clawden up`

`clawden up` is the **orchestration** command — it starts runtimes as defined in `clawden.yaml` for multi-runtime setups. Unlike `run`, it is opinionated about how runtimes start (daemon mode, Docker containers, auto-restart).

`clawden up` **keeps** the implicit `daemon`/`gateway` behavior. When you define a runtime in `clawden.yaml` and run `clawden up`, ClawDen is the orchestrator — it decides how to start things. This is the same distinction as:

- `docker run nginx` — you control the command (passthrough)
- `docker compose up` — compose controls the command (orchestrated)

Implementation: Move the current `runtime_start_args()` logic into the `up` command path only, or into a new `runtime_default_start_mode()` function that `up` uses but `run` does not.

### Impact on Docker Mode

When `clawden run` uses Docker mode (via `mode: docker` in config), the container entrypoint already handles subcommand defaulting. No change needed for Docker mode — the entrypoint script in `docker/entrypoint.sh` is the runtime's own default, not ClawDen's.

### Migration / Backward Compatibility

This is a **breaking change** for users who rely on `clawden run zeroclaw` starting in daemon mode. Mitigation:

1. **Phase 1 — Warn (this release):** When `runtime_start_args()` would have injected a subcommand, print a deprecation warning:
   ```
   ⚠ Implicit subcommand "daemon" is deprecated and will be removed in v0.4.
     Write `clawden run zeroclaw daemon` explicitly.
     See: https://clawden.dev/docs/run-passthrough
   ```
   Still inject the subcommand for now.

2. **Phase 2 — Remove (v0.4):** Delete `runtime_start_args()`. Clean passthrough is the default. Show the hint on non-zero exit.

Alternatively, if the user base is small enough (pre-1.0), skip Phase 1 and go straight to clean passthrough with the hint system.

## Implementation Checklist

### Phase 1 (Deprecation Warning - Skipped by Decision)

- [x] Add deprecation warning in `run.rs` when `start_args` is non-empty (skipped intentionally; Phase 2 implemented directly pre-1.0)
- [x] Log the full command being executed (with `debug!`) so users can see what's happening
- [x] Add `runtime_subcommand_hints()` function to `clawden-core/src/install.rs`
- [x] Update `clawden run --help` to document passthrough semantics

### Phase 2 (Clean Passthrough)

- [x] Remove `runtime_start_args()` from `clawden-core/src/install.rs`
- [x] Remove `start_args` field from `InstalledRuntime` struct (or keep as always-empty)
- [x] Remove `runtime_supported_extra_args()` and `validate_runtime_args()`
- [x] Update `run.rs` to not prepend `start_args` to the command
- [x] Move default subcommand logic into `up.rs` only (for orchestrated starts)
- [x] Add non-zero exit hint system using `runtime_subcommand_hints()`
- [x] Ensure `--config-dir` is injected after the first positional arg (subcommand)
- [x] Update integration tests to pass explicit subcommands
- [x] Update `clawden run` documentation / help text
- [x] Audit `docker/entrypoint.sh` — confirm it handles bare invocation correctly (no ClawDen-side injection needed)

## Alternatives Considered

### A. Keep implicit subcommands, add `--raw` flag for passthrough

Rejected. Defaults matter — the common case should be the clean one. A `--raw` flag is an escape hatch that admits the default is wrong.

### B. Add a `clawden exec` command for passthrough, keep `run` as-is

Rejected. Two commands for "run a runtime" is confusing. `run` is the right name — it should do the right thing.

### C. Detect subcommands from runtime `--help` output automatically

Over-engineered. Parsing --help output is fragile across runtimes and versions. The hint registry is simpler and sufficient.

## References

- Spec 033: Product Positioning — `uv run` analogy, transparent execution model
- Spec 034: CLI Runtime Ergonomics — argument separation, config injection
- Spec 035: Run Command UX Polish — credential validation, error messages
- `uv run` docs: https://docs.astral.sh/uv/concepts/projects/run/
- `runtime_start_args()`: `crates/clawden-core/src/install.rs:564`
- `exec_run()`: `crates/clawden-cli/src/commands/run.rs:46`