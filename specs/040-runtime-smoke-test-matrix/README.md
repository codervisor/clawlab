---
status: planned
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
updated_at: 2026-03-05T02:04:00.000000000Z
---

# Runtime Smoke Test Matrix — End-to-End Validation of All Supported Runtimes

## Overview

ClawDen supports five claw runtimes (OpenClaw, ZeroClaw, PicoClaw, NanoClaw, OpenFang), each with a different language, install mechanism, config format, and startup protocol. Currently there is no automated test that validates the full `clawden run <runtime>` lifecycle — from install through config generation to successful startup — for every runtime. This spec documents the blockers discovered during manual smoke testing and defines an automated test matrix to catch regressions.

## Context

### Problem

Running `clawden run <runtime> --channel telegram` against each supported runtime reveals multiple blockers that prevent successful startup. These issues were discovered on 2026-03-05 against the current `main` branch:

| Runtime | Install | Config Gen | Startup | Status |
|---------|---------|------------|---------|--------|
| **ZeroClaw** | ✅ Pre-installed (0.1.7) | ✅ TOML config generated | ✅ `daemon` subcommand starts successfully | **Working** (needs subcommand) |
| **PicoClaw** | ❌ Fails — requires `7z` | — | — | **Blocked: missing system dep** |
| **OpenClaw** | ✅ Installs via npm (slow ~60s) | ⚠ No config-dir support | ⚠ Starts but health check unresponsive | **Partially working** |
| **NanoClaw** | ✅ Clones + pnpm install | — | ❌ `Cannot find module 'dist/index.js'` | **Blocked: build step skipped** |
| **OpenFang** | ✅ Downloads binary (0.3.17) | ❌ `--config-dir` rejected | ❌ `unexpected argument '--config-dir'` | **Blocked: wrong CLI flag** |

### Blocker Details

#### B1: PicoClaw — `7z` not available on host (Direct Install)

**Symptom**: `Error: Tool '7z' is required for direct install. Install it first (hint: p7zip).`

**Root cause**: PicoClaw's release artifacts are distributed as `.7z` archives. The direct install path (`install_picoclaw`) requires the `7z` command (from `p7zip-full`), which is not commonly installed on developer machines. The Docker image pre-installs `p7zip-full`, so Docker mode works.

**Impact**: Any user running `clawden run picoclaw` without Docker and without `p7zip` gets a hard failure with no fallback.

**Potential fixes**:
1. Add `p7zip` to the auto-install prerequisites or offer to install it
2. Request PicoClaw upstream to also publish `.tar.gz` or `.zip` archives
3. Bundle a minimal 7z extractor or use a Rust lzma/7z decompression crate (e.g., `sevenz-rust`) to eliminate the system dependency
4. Fall back to Docker mode when the required tool is missing

#### B2: NanoClaw — Build step skipped during install

**Symptom**: `Error: Cannot find module '/home/.../.clawden/runtimes/nanoclaw/main/nanoclaw-src/dist/index.js'`

**Root cause**: The `install_nanoclaw` function runs `pnpm install --prod --ignore-scripts`, which skips the TypeScript compilation (`tsc`) build step. NanoClaw's `package.json` defines `"main": "dist/index.js"` and `"build": "tsc"`, but `dist/` is not checked into the repository — it must be compiled from `src/`.

The `--ignore-scripts` flag prevents the `prepare` hook from running, and `--prod` skips dev dependencies (which likely include `typescript`). The result is a cloned repo with source files but no compiled output.

**Impact**: NanoClaw always fails to start after a fresh direct install. The runtime is completely non-functional in direct mode.

**Potential fixes**:
1. Run `pnpm install` (without `--prod`) followed by `pnpm run build` to compile TypeScript
2. Use `pnpm install --ignore-scripts` (without `--prod`) then explicitly `pnpm run build`
3. Check if NanoClaw publishes pre-built release tarballs that include `dist/`
4. Use `tsx` (TypeScript executor) instead of `node` for the launcher: `tsx src/index.ts` instead of `node dist/index.js`

#### B3: OpenFang — `--config-dir` flag not supported

**Symptom**: `error: unexpected argument '--config-dir' found; tip: a similar argument exists: '--config'`

**Root cause**: ClawDen's `inject_config_dir_arg` injects `--config-dir <path>` for runtimes that report `runtime_supports_config_dir() == true`. OpenFang is listed as supporting config-dir, but the actual OpenFang binary (v0.3.17) accepts `--config <file>` (a single config file path), not `--config-dir <directory>`.

The mismatch is in `runtime_supports_config_dir()` returning `true` for `"openfang"`, and `inject_config_dir_arg` unconditionally using the `--config-dir` flag name for all runtimes.

**Impact**: OpenFang always fails to start via `clawden run openfang`. The config is generated correctly (TOML), but the injection mechanism uses the wrong flag.

**Potential fixes**:
1. Map OpenFang to `--config <path>/config.toml` instead of `--config-dir <path>`
2. Add per-runtime config flag customization to `inject_config_dir_arg`
3. Use OpenFang's `OPENFANG_CONFIG` environment variable instead of a CLI flag

#### B4: ZeroClaw — Requires explicit subcommand

**Symptom**: `error: 'zeroclaw' requires a subcommand but one was not provided`

**Root cause**: `clawden run` in direct mode does not inject default start args. The `runtime_default_start_args_for_up()` function provides defaults (`["daemon"]` for ZeroClaw), but this is only used by `clawden up`, not `clawden run`. In run mode, the user must pass the subcommand (e.g., `clawden run zeroclaw daemon`).

**Impact**: Bare `clawden run zeroclaw` fails with an unhelpful error. The hint text is shown, but the user has to know to add a subcommand. This is by design (spec 033/036 — run is transparent passthrough), but the UX could be improved.

**Status**: Working as designed — `clawden run zeroclaw daemon` succeeds. This is a UX issue, not a blocker.

#### B5: OpenClaw — Health check unresponsive

**Symptom**: `⚠ openclaw started (pid ...) but health check not responding`

**Root cause**: OpenClaw starts but the health check probe doesn't get a response within the timeout. This may be due to slow Node.js startup, OpenClaw not exposing a health endpoint on the expected port, or the adapter's health check URL being incorrect.

**Impact**: OpenClaw appears to run (the process stays alive) but ClawDen reports a degraded state. Functionality may or may not work depending on the root cause.

**Status**: Partially working — needs investigation into the health check endpoint configuration.

### Runtime Install Prerequisites Summary

| Runtime | Language | Install Method | Required System Tools | Config Format |
|---------|----------|---------------|----------------------|---------------|
| ZeroClaw | Rust | GitHub release tarball | `curl`, `tar` | TOML |
| PicoClaw | Go | GitHub release 7z archive | `curl`, **`7z`** (p7zip) | JSON |
| OpenClaw | TypeScript | npm global install | `node`, `npm` | Env vars |
| NanoClaw | TypeScript | Git clone + pnpm | `git`, `node`, `pnpm`, **`typescript`** (dev dep) | Env vars |
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

1. **B3 — OpenFang config flag** (Quick fix): Change config injection to use `--config <file>` for OpenFang instead of `--config-dir`
2. **B2 — NanoClaw build step** (Medium fix): Add `pnpm run build` after dependency install, or switch to `tsx` launcher
3. **B1 — PicoClaw 7z dependency** (Medium fix): Use `sevenz-rust` crate or fall back to Docker when `7z` is missing
4. **B5 — OpenClaw health check** (Investigation): Determine correct health endpoint and timeout
5. **B4 — ZeroClaw subcommand UX** (Optional): Consider auto-injecting `daemon` in `clawden run` when no subcommand is provided

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

- [ ] All 5 runtimes have corresponding fake-binary integration tests in `runtime_smoke.rs`
- [ ] Tests validate install prereqs, config generation, config injection flags, and startup args
- [ ] Blockers B1–B3 are fixed (PicoClaw 7z fallback, NanoClaw build step, OpenFang config flag)
- [ ] B5 (OpenClaw health check) root cause is identified and documented
- [ ] CI runs the smoke test matrix on every PR
- [ ] `cargo test -p clawden-cli --test runtime_smoke` passes with all tests green
