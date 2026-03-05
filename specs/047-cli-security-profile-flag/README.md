---
status: planned
created: 2026-03-05
priority: medium
tags:
- cli
- security
- developer-experience
- run
- up
- config
depends_on:
- 039-zeroclaw-security-defaults-compat
created_at: 2026-03-05T09:27:16.632450226Z
updated_at: 2026-03-05T09:27:16.632450226Z
---

# CLI --security-profile Flag for Runtime Security Override

## Overview

Spec 039 introduced the "managed" security profile that ClawDen automatically injects when launching runtimes, relaxing resource limits, seccomp, capability dropping, and sandboxing. However, there is **no CLI flag or environment variable** to override this profile at launch time.

Users who want to run with `strict` (full runtime security defaults) or `permissive` (no restrictions) must edit `clawden.yaml` — there's no quick ergonomic override for one-off runs or testing.

### Use Cases

- **Development/debugging**: Run with `permissive` to rule out security-layer issues during troubleshooting
- **Hardened deployment**: Force `strict` profile to test runtime behavior with full security defaults
- **CI/smoke tests**: Parametrize security profile in test matrices without editing config files
- **Quick override**: Change profile for a single `clawden run` without modifying project config

## Design

### 1. CLI Flag: `--security-profile`

Add `--security-profile <strict|managed|permissive>` to both `Run` and `Up` commands in `cli.rs`:

```rust
/// Override security profile (strict, managed, permissive).
/// Defaults to "managed" when omitted.
#[arg(long, value_parser = ["strict", "managed", "permissive"])]
security_profile: Option<String>,
```

### 2. Environment Variable: `CLAWDEN_SECURITY_PROFILE`

When the CLI flag is not provided, check `CLAWDEN_SECURITY_PROFILE` env var as a fallback. This enables CI and container-level overrides without flag passing.

Precedence: CLI flag > env var > clawden.yaml `security.profile` > default (`managed`).

### 3. Wiring

- **`run.rs`**: Read `security_profile` arg, pass to config generation and direct-mode env setup
- **`up.rs`**: Read `security_profile` arg, pass to `inject_security_profile()` in `config_gen.rs`
- **`config_gen.rs`**: `inject_security_profile()` already sets `profile = "managed"` — make it accept an optional override parameter. When `strict`, skip the relaxation injection entirely. When `permissive`, inject the same relaxation plus disable all remaining security fields
- **`docker_runtime.rs`**: When `strict`, omit `--security-opt seccomp=unconfined` and `--ulimit` overrides from Docker args

### 4. Behavior Matrix

| Profile | rlimits | seccomp | capabilities | sandbox | Docker ulimit/seccomp |
|---------|---------|---------|-------------|---------|----------------------|
| `strict` | Runtime defaults | Runtime default | Runtime drops caps | Runtime sandbox | Not overridden |
| `managed` | Inherited (0) | Disabled | Not dropped | Delegated to ClawDen | `nofile=65536`, `seccomp=unconfined` |
| `permissive` | Inherited (0) | Disabled | Not dropped | Disabled entirely | `nofile=65536`, `seccomp=unconfined` |

## Plan

- [ ] Add `--security-profile` arg to `Run` command in `cli.rs`
- [ ] Add `--security-profile` arg to `Up` command in `cli.rs`
- [ ] Read `CLAWDEN_SECURITY_PROFILE` env var as fallback in `run.rs` and `up.rs`
- [ ] Thread profile override into `inject_security_profile()` in `config_gen.rs`
- [ ] Skip security relaxation injection when profile is `strict`
- [ ] Conditionally omit Docker security overrides in `docker_runtime.rs` for `strict` mode
- [ ] Add CLI parse tests for the new flag
- [ ] Update `clawden run --help` / `clawden up --help` output

## Test

- [ ] `clawden run --security-profile strict zeroclaw` does not inject `[security]` TOML relaxation
- [ ] `clawden run --security-profile permissive zeroclaw` injects full relaxation
- [ ] `clawden run zeroclaw` (no flag) defaults to `managed` behavior (existing behavior)
- [ ] `CLAWDEN_SECURITY_PROFILE=strict clawden run zeroclaw` applies strict without flag
- [ ] CLI flag takes precedence over env var
- [ ] `clawden up --security-profile strict` applies to all runtimes in clawden.yaml
- [ ] Invalid values (e.g. `--security-profile foo`) are rejected by clap

## Notes

- The `permissive` vs `managed` distinction is subtle — `managed` still delegates sandbox to ClawDen, while `permissive` disables sandboxing outright. This matters for tool execution isolation.
- For Docker mode, `strict` means the runtime's own container-level security opts are used; ClawDen does not override them.
- This does not affect the `allowlist` field — channel authorization is orthogonal to runtime security profile.