---
status: in-progress
created: 2026-03-05
priority: high
tags:
- testing
- runtime
- smoke-test
- direct-install
- blockers
- ci
created_at: 2026-03-05T02:04:00.000000000Z
updated_at: 2026-03-05T02:56:00.000000000Z
---

# Runtime Smoke Test Matrix — End-to-End Validation of All Supported Runtimes

## Overview

ClawDen supports five claw runtimes (OpenClaw, ZeroClaw, PicoClaw, NanoClaw, OpenFang), each with a different language, install mechanism, config format, and startup protocol. Currently there is no automated test that validates the full `clawden run <runtime>` lifecycle — from install through config generation to successful startup — for every runtime. This spec documents the blockers discovered during manual smoke testing and defines an automated test matrix to catch regressions.

## Context

### Problem

Running `clawden run <runtime> --channel telegram` against each supported runtime reveals multiple blockers that prevent successful startup. These issues were discovered on 2026-03-05 against the current `main` branch:

| Runtime | Install | Config Gen | Startup | Status |
|---------|---------|------------|---------|--------|
| **ZeroClaw** | ✅ Pre-installed (0.1.7) | ✅ TOML config generated | ✅ `daemon` subcommand starts successfully | **Fixed** (smart default subcommand) |
| **PicoClaw** | ✅ **Fixed** — uses `sevenz-rust` crate | ✅ JSON config generated | ✅ Starts with `gateway` | **Fixed** |
| **OpenClaw** | ✅ Installs via npm (slow ~60s) | ⚠ No config-dir support | ⚠ Health check port corrected (18789) | **Partially working** |
| **NanoClaw** | ✅ **Fixed** — builds TS + native modules | ⚠ No config-dir support | ✅ Database initializes, exits without channel creds (expected) | **Fixed** |
| **OpenFang** | ✅ Downloads binary (0.3.17) | ✅ **Fixed** — uses `--config <file>` | ✅ Kernel boots, agents spawn | **Fixed** |

### Blocker Details

#### B1: PicoClaw — `7z` not available on host (Direct Install) — ✅ FIXED

**Symptom**: `Error: Tool '7z' is required for direct install. Install it first (hint: p7zip).`

**Root cause**: PicoClaw's release artifacts are distributed as `.7z` archives. The direct install path (`install_picoclaw`) required the `7z` command (from `p7zip-full`), which is not commonly installed on developer machines.

**Fix applied**: Replaced system `7z` command with the `sevenz-rust` crate (v0.6) for native Rust 7z extraction. No system dependency needed.

**Files changed**:
- `Cargo.toml` — added `sevenz-rust = "0.6"` to workspace deps
- `crates/clawden-core/Cargo.toml` — added `sevenz-rust` dep
- `crates/clawden-core/src/install.rs` — replaced `ensure_command_available("7z")` + shell `7z x` with `sevenz_rust::decompress_file()`

#### B2: NanoClaw — Build step skipped during install — ✅ FIXED

**Symptom**: `Error: Cannot find module '/home/.../.clawden/runtimes/nanoclaw/main/nanoclaw-src/dist/index.js'`

**Root cause**: The `install_nanoclaw` function ran `pnpm install --prod --ignore-scripts`, which skipped TypeScript compilation and native module compilation (better-sqlite3). NanoClaw requires both `dist/index.js` (from `tsc`) and `better_sqlite3.node` (native addon via `prebuild-install`).

**Fix applied** (three changes):
1. Removed `--prod` and `--ignore-scripts` flags from `pnpm install` so dev deps and install scripts run
2. Added explicit `pnpm run build` step after install to compile TypeScript
3. Added `rebuild_native_modules()` function that walks `node_modules/.pnpm` for packages with `binding.gyp`, detects missing `.node` files, and runs `npx prebuild-install` (with `node-gyp` fallback) to build them
4. Added validation that `dist/index.js` exists after build

**Files changed**:
- `crates/clawden-core/src/install.rs` — updated `install_nanoclaw`, added `rebuild_native_modules()` and `walkdir()` helper

#### B3: OpenFang — `--config-dir` flag not supported — ✅ FIXED

**Symptom**: `error: unexpected argument '--config-dir' found; tip: a similar argument exists: '--config'`

**Root cause**: `inject_config_dir_arg` unconditionally used `--config-dir` for all runtimes, but OpenFang (v0.3.17) accepts `--config <file>` (a single config file path).

**Fix applied**: Added OpenFang-specific branch in `inject_config_dir_arg` to pass `--config <config_dir>/config.toml` instead of `--config-dir <config_dir>`.

**Files changed**:
- `crates/clawden-cli/src/commands/config_gen.rs` — updated `inject_config_dir_arg` and test assertion

#### B4: ZeroClaw — Requires explicit subcommand — ✅ FIXED

**Symptom**: `error: 'zeroclaw' requires a subcommand but one was not provided`

**Root cause**: `clawden run` in direct mode did not inject default start args. The `runtime_default_start_args()` function provides defaults (`["daemon"]` for ZeroClaw), but was only used by `clawden up`, not `clawden run`.

**Fix applied**: `clawden run` now injects smart default subcommands when no extra args are provided. The defaults are the same ones `clawden up` uses (e.g., `daemon` for ZeroClaw, `gateway` for PicoClaw, `start` for OpenFang). An info message is printed: `ℹ No subcommand specified — using default: zeroclaw daemon`. Users can still override by passing an explicit subcommand.

**Files changed**:
- `crates/clawden-core/src/install.rs` — renamed `runtime_default_start_args_for_up` → `runtime_default_start_args`, updated doc comment
- `crates/clawden-core/src/lib.rs` — updated re-export
- `crates/clawden-cli/src/commands/run.rs` — inject default subcommand when `extra_args` is empty
- `crates/clawden-cli/src/commands/up.rs` — updated to use renamed function
- `crates/clawden-cli/tests/run_ergonomics.rs` — added tests for smart default injection

#### B5: Health check ports incorrect — ✅ FIXED

**Symptom**: `⚠ openclaw started (pid ...) but health check not responding`

**Root cause**: `runtime_health_url()` in `process.rs` hardcoded incorrect ports for multiple runtimes:
- ZeroClaw: used port 3000 (was coincidentally answered by leanspec-http), actual port is 42617
- OpenClaw: used port 3001, actual port is 18789
- OpenFang: used port 4200, actual port is 50051

**Fix applied**: Corrected all three port mappings to match the actual default ports from the runtime adapter metadata and verified against running binaries. Also updated OpenFang adapter metadata (`default_port: 50051`) and default start args (`"start"` instead of `"daemon"`).

**Files changed**:
- `crates/clawden-core/src/process.rs` — fixed `runtime_health_url()` port mappings
- `crates/clawden-core/src/install.rs` — fixed `runtime_default_start_args_for_up` and `runtime_subcommand_hints` for OpenFang
- `crates/clawden-adapters/src/openfang.rs` — fixed `default_port` metadata
- `crates/clawden-cli/tests/run_ergonomics.rs` — increased fake runtime sleep to outlast health check window

### Runtime Install Prerequisites Summary

| Runtime | Language | Install Method | Required System Tools | Config Format |
|---------|----------|---------------|----------------------|---------------|
| ZeroClaw | Rust | GitHub release tarball | `curl`, `tar` | TOML |
| PicoClaw | Go | GitHub release 7z archive | `curl` (native 7z extraction via `sevenz-rust`) | JSON |
| OpenClaw | TypeScript | npm global install | `node`, `npm` | Env vars |
| NanoClaw | TypeScript | Git clone + pnpm | `git`, `node`, `pnpm` | Env vars |
| OpenFang | Rust | GitHub release tarball | `curl` | TOML |

## Proposed Solution

### 1. Automated Smoke Test Suite

Add a new integration test file `crates/clawden-cli/tests/runtime_smoke.rs` that validates each runtime's install → config → startup lifecycle using mock/fake binaries (similar to how `run_ergonomics.rs` creates fake zeroclaw executables).

**Test matrix per runtime**:
- `test_{runtime}_install_prerequisites` — Verify required tools are checked before download
- `test_{runtime}_config_generation` — Verify config file is generated in the correct format (TOML/JSON/env)
- `test_{runtime}_config_injection` — Verify the correct CLI flag or env var is used to pass config
- `test_{runtime}_startup_args` — Verify default subcommands and arg passthrough work
- `test_{runtime}_health_check` — Verify health probe URL and expected response

**Fake runtime approach**:
Each test creates a shell script that mimics the runtime's expected interface:
- Accepts the same CLI flags (`--config-dir`, `--config`, subcommands)
- Writes received args and env vars to a dump file for assertion
- Listens on the expected port for health check validation (optional)
- Exits cleanly after startup verification

### 2. Fix Blockers (Priority Order)

1. **B3 — OpenFang config flag** ✅ Done: Changed config injection to use `--config <file>` for OpenFang
2. **B2 — NanoClaw build step** ✅ Done: Added `pnpm run build` + native module rebuild after install
3. **B1 — PicoClaw 7z dependency** ✅ Done: Used `sevenz-rust` crate — no system `7z` needed
4. **B5 — Health check ports** ✅ Done: Corrected ports for ZeroClaw (42617), OpenClaw (18789), OpenFang (50051)
5. **B4 — ZeroClaw subcommand UX** ✅ Done: Smart default subcommand injection when no args provided

### 3. CI Integration

Add a GitHub Actions job that runs the smoke test matrix on every PR:

```yaml
runtime-smoke:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - name: Install Rust
      uses: dtolnay/rust-toolchain@stable
    - name: Run runtime smoke tests
      run: cargo test -p clawden-cli --test runtime_smoke --quiet
```

## Test Cases

### Unit Tests (in `clawden-core`)

- [ ] `runtime_supports_config_dir` returns correct values for all 5 runtimes
- [ ] `runtime_default_start_args_for_up` returns expected defaults
- [ ] `runtime_subcommand_hints` covers all runtimes
- [ ] `ensure_command_available` returns correct error messages with install hints

### Integration Tests (in `clawden-cli`)

- [ ] Fake ZeroClaw: accepts `--config-dir`, `daemon` subcommand, writes env dump
- [ ] Fake PicoClaw: accepts `--config-dir`, `gateway` subcommand, reads JSON config
- [ ] Fake OpenClaw: accepts env vars, starts without config-dir flag
- [ ] Fake NanoClaw: accepts env vars, starts without config-dir flag
- [ ] Fake OpenFang: accepts `--config <file>`, reads TOML config, starts with subcommand

### Smoke Tests (requires real runtimes — CI optional)

- [ ] `clawden install zeroclaw` succeeds with only `curl` + `tar`
- [ ] `clawden install picoclaw` fails gracefully when `7z` is missing
- [ ] `clawden install openclaw` succeeds with `node` + `npm`
- [ ] `clawden install nanoclaw` produces runnable `dist/index.js`
- [ ] `clawden install openfang` succeeds with only `curl`
- [ ] `clawden run zeroclaw daemon` starts and passes health check
- [ ] `clawden run picoclaw gateway` starts and passes health check
- [ ] `clawden run openclaw` starts and passes health check
- [ ] `clawden run nanoclaw` starts and passes health check
- [ ] `clawden run openfang start` starts and passes health check

## Dependencies

- Spec 010 — Claw Runtime Interface (adapter trait)
- Spec 022 — Direct Install
- Spec 031 — Direct Mode Config Injection
- Spec 032 — OpenFang Runtime Adapter
- Spec 033 — Product Positioning (run vs up semantics)
- Spec 039 — ZeroClaw Security Defaults

## Acceptance Criteria

- [x] Blockers B1–B3 are fixed (PicoClaw 7z fallback, NanoClaw build step, OpenFang config flag)
- [x] B5 (health check ports) root cause identified and fixed
- [ ] All 5 runtimes have corresponding fake-binary integration tests in `runtime_smoke.rs`
- [ ] Tests validate install prereqs, config generation, config injection flags, and startup args
- [ ] CI runs the smoke test matrix on every PR
- [ ] `cargo test -p clawden-cli --test runtime_smoke` passes with all tests green
