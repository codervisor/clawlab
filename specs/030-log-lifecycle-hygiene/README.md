---
status: in-progress
created: 2026-03-02
priority: high
tags:
- bug
- cli
- logs
- process-manager
depends_on:
- 027-docker-compose-ux
created_at: 2026-03-02T08:34:22.911298893Z
updated_at: 2026-03-02T09:04:49.613153208Z
transitions:
- status: in-progress
  at: 2026-03-02T09:04:49.613153208Z
---

# Log Lifecycle Hygiene — Stream Offset, Rotation & Start-Arg Safety

## Overview

`clawden up` replays the entire log history from all previous runs because `stream_logs()` reads from byte 0 and log files are never truncated. This causes confusing output — including stale help text from prior failed launches — mixed in with current session output. A related safety gap in `ensure_installed_runtime()` can produce empty `start_args`, launching runtimes with no subcommand.

## Context

### Bug 1: `stream_logs()` reads from byte 0

In `ProcessManager::stream_logs()` (`crates/clawden-core/src/process.rs`), the offset map is initialized empty:

```rust
let mut offsets: HashMap<String, usize> = HashMap::new();
// ...
let offset = offsets.entry(runtime.clone()).or_insert(0usize);
```

Every call to `stream_logs` replays the full content of `~/.clawden/logs/{runtime}.log` from the beginning, regardless of when the current session started. Users see output from hours/days-old runs interleaved with current output.

### Bug 2: Log files accumulate forever

`start_direct_with_env_and_project()` opens the log file with `OpenOptions::new().create(true).append(true)`, so content from every run accumulates indefinitely. There is no truncation or rotation on new `clawden up` invocations.

### Bug 3: `ensure_installed_runtime()` can produce empty `start_args`

In `crates/clawden-cli/src/util.rs`:

```rust
let start_args = installer
    .list_installed()?
    .into_iter()
    .find(|row| row.runtime == runtime)
    .map(|row| row.start_args)
    .unwrap_or_default();  // empty Vec if runtime not found in list
```

If `list_installed()` doesn't find the runtime (e.g. symlink issue), `start_args` becomes `[]`. The runtime launches with no subcommand, prints help text to the log, and exits. That help text then persists in the log file and is replayed on every subsequent `clawden up`.

### Impact

Users running `clawden up` see confusing output: help text from a prior broken launch followed by current daemon output, with no indication of session boundaries. The log file grows unbounded.

### Related

- **027-docker-compose-ux** (complete): Added `stream_logs` and log prefixing but didn't address offset initialization or file rotation
- **029-docker-mode-config-injection** (planned): Docker mode adapter issues — separate concern

## Design

### 1. Start `stream_logs` from current EOF

When `stream_logs()` initializes, seed each runtime's offset to the current file size instead of 0:

```rust
let mut offsets: HashMap<String, usize> = HashMap::new();
for (runtime, log_path) in &watched {
    if let Ok(meta) = fs::metadata(log_path) {
        offsets.insert(runtime.clone(), meta.len() as usize);
    }
}
```

This ensures only new output written after `stream_logs()` is called gets streamed.

### 2. Truncate log file on new session start

In `start_direct_with_env_and_project()`, replace `.append(true)` with `.truncate(true)` (or `.write(true).truncate(true)`) when opening the log file. Each `clawden up` session starts with a clean log. Old logs can optionally be rotated to `{runtime}.log.1`.

### 3. Guard `ensure_installed_runtime()` against empty `start_args`

Fall back to `runtime_start_args()` when `list_installed()` doesn't find the runtime, instead of `unwrap_or_default()`:

```rust
let start_args = installer
    .list_installed()?
    .into_iter()
    .find(|row| row.runtime == runtime)
    .map(|row| row.start_args)
    .unwrap_or_else(|| clawden_core::runtime_start_args(runtime));
```

This requires exposing `runtime_start_args` as `pub` from `clawden-core::install`.

## Plan

- [x] Seed `stream_logs()` offsets to current file size instead of 0
- [x] Truncate (or rotate) runtime log files at the start of each `clawden up` / `clawden run` session
- [x] Fix `ensure_installed_runtime()` to fall back to `runtime_start_args()` instead of empty vec
- [x] Expose `runtime_start_args()` as pub from `clawden-core::install`
- [x] Add test: `stream_logs` with pre-existing log content only streams new lines
- [ ] Add test: `ensure_installed_runtime` returns correct `start_args` even when `list_installed` misses the entry

## Test

- [ ] `clawden up` after a previous session only shows output from the current run (no stale replay)
- [ ] Log file is truncated/rotated on each new `clawden up` invocation
- [ ] A runtime whose symlink is broken still launches with the correct subcommand (e.g. `daemon` for zeroclaw)
- [ ] `clawden logs -f` after `clawden up -d` streams only current-session output
- [ ] Direct mode behavior is unchanged aside from the log fixes (no regression)