---
status: planned
created: 2026-03-05
priority: high
tags:
- security
- zeroclaw
- config
- rlimit
- sandbox
- compatibility
- developer-experience
- openclaw
- openfang
- multi-runtime
created_at: 2026-03-05T01:56:04.595663369Z
updated_at: 2026-03-05T09:08:00.189715585Z
---

# Runtime Security Defaults Compatibility — Relaxed Limits for ClawDen-Managed Execution

## Overview

Multiple claw runtimes ship with strict security defaults: aggressive resource limits (`rlimit`), restrictive seccomp profiles, capability dropping, and sandboxed tool execution. While these defaults are appropriate for standalone multi-tenant deployments, they cause failures and degraded performance when runtimes are launched via ClawDen — where ClawDen itself provides the trust boundary and orchestration layer.

This spec originally targeted ZeroClaw (≥0.1.7) but has been extended to cover **OpenClaw** and **OpenFang**, which have their own security constraints that conflict with ClawDen-managed execution.

## Context

### Problem

When `clawden run <runtime>` or `clawden up` launches a runtime, users hit unexpected failures:

1. **Memory limits (`RLIMIT_AS` / `RLIMIT_DATA`)**: ZeroClaw's default memory cap (e.g., 512MB) is too low for LLM-heavy workloads that buffer large context windows, tool outputs, or multi-turn conversation history. The runtime OOM-kills itself or child tool processes.

2. **File descriptor limits (`RLIMIT_NOFILE`)**: The default cap (e.g., 256 FDs) is insufficient when runtimes connect to multiple channels simultaneously (Telegram polling + Discord gateway + Slack socket mode), each requiring persistent connections plus the LLM provider HTTP client pool.

3. **Seccomp profile**: ZeroClaw's default seccomp filter blocks syscalls used by common tools — `clone3` (needed by modern glibc), `io_uring_*` (used by some async runtimes), `execve` restrictions that interact poorly with ClawDen's sandbox wrapper (`clawden-sandbox` / bwrap).

4. **Capability dropping**: ZeroClaw drops `CAP_NET_RAW` by default, which breaks ICMP-based health checks and network diagnostic tools. It also drops `CAP_SYS_PTRACE`, which prevents ClawDen's process monitoring from attaching to inspect stuck child processes.

5. **Sandboxed tool execution conflicts**: ZeroClaw has its own sandboxing layer for tool calls, which conflicts with ClawDen's `clawden-sandbox` (bwrap-based). Running bwrap-inside-bwrap fails with `EPERM` because the inner namespace creation is denied by the outer namespace's seccomp filter.

6. **OpenClaw subprocess isolation**: OpenClaw spawns Node.js worker threads for tool execution with its own process isolation layer. When running under ClawDen's sandbox, the worker thread creation can fail due to conflicting namespace restrictions.

7. **OpenFang gRPC server binding**: OpenFang's default security config restricts binding to localhost-only with TLS required. When ClawDen manages the network layer (especially in Docker mode), these restrictions prevent health probes and inter-service communication on the container network.

### Why This Matters for ClawDen

ClawDen's architecture (spec 024, spec 033) positions it as the **trust boundary and orchestration layer**:

- ClawDen provides its own sandbox (`clawden-sandbox`, bubblewrap) for tool execution isolation (spec 024)
- ClawDen manages process lifecycle, health checks, and process monitoring (spec 031)
- ClawDen handles credential injection and config generation — it already controls the security perimeter (specs 025, 029, 031)
- In Docker mode, the container itself provides isolation; ZeroClaw's inner restrictions are redundant

When ZeroClaw independently enforces strict limits, it creates a **double-sandboxing** problem where two layers conflict and neither works correctly.

### Affected Failure Modes

| Symptom | Root Cause | Impact |
|---------|-----------|--------|
| OOM crash during long conversations | `RLIMIT_AS` too low for LLM context | Runtime dies, user loses session |
| "Too many open files" with multi-channel | `RLIMIT_NOFILE` too low | Channel connections drop |
| Tool execution `EPERM` errors | Nested bwrap (ClawDen sandbox inside ZeroClaw sandbox) | Tools fail silently |
| Health check probe fails | `CAP_NET_RAW` dropped, ICMP blocked | ClawDen thinks runtime is down, triggers restart loop |
| `clone3` SIGSYS in tool subprocess | Seccomp blocks modern glibc syscalls | Cryptic crash in tool child processes |

### Affected Runtimes

| Runtime | Has own security limits | Conflicts with ClawDen | Status |
|---------|------------------------|----------------------|--------|
| ZeroClaw | Yes (strict defaults) | **Yes** | **In scope** |
| OpenClaw | Yes (worker isolation, env sandboxing) | **Yes** | **In scope** |
| OpenFang | Yes (gRPC TLS, bind restrictions) | **Yes** | **In scope** |
| NullClaw | Likely (similar codebase to ZeroClaw) | Probable | Future |
| NanoClaw | Minimal (env-only) | Minimal | Out of scope |
| PicoClaw | Minimal (Go defaults) | Minimal | Out of scope |

## Solution

### Strategy: "ClawDen-Managed" Security Profile

When a runtime is launched by ClawDen (vs. standalone), inject a relaxed security profile that defers trust-boundary enforcement to ClawDen. Runtimes still run their own application-level security (allowlists, auth), but resource limits and process isolation are managed by ClawDen's outer layer.

The injection mechanism is format-aware: TOML `[security]` for ZeroClaw/OpenFang, env vars for OpenClaw, and the universal `CLAWDEN_MANAGED=1` env var for all runtimes.

### 1. Security Profile Config Injection (`config_gen.rs`)

Add `inject_security_profile()` that emits security sections for all runtimes with security concerns. The function is format-aware, following the existing proxy injection pattern.

**ZeroClaw / OpenFang (TOML):**
```toml
[security]
profile = "managed"           # vs. "strict" (default)
rlimit_as = 0                 # 0 = inherit from parent (no override)
rlimit_nofile = 0             # 0 = inherit from parent
rlimit_nproc = 0              # 0 = inherit from parent
seccomp = "disabled"          # ClawDen's sandbox handles this
drop_capabilities = false     # ClawDen manages capabilities
sandbox_tools = false         # Use ClawDen's clawden-sandbox instead
```

**OpenFang additional fields (TOML):**
```toml
[security]
tls_required = false          # ClawDen manages TLS termination
bind_address = "0.0.0.0"      # Allow binding on all interfaces for container networking
```

**OpenClaw (env vars):**
```bash
OPENCLAW_WORKER_ISOLATION=none      # Disable worker thread isolation
OPENCLAW_SANDBOX_MODE=external      # Defer to ClawDen sandbox
```

**Behavior:**
- Only injected when ClawDen launches the runtime (presence of `--config-dir` flag or `CLAWDEN_MANAGED=1` env var)
- Not injected for standalone usage
- User can override any field via `clawden.yaml` config overrides (same merge pattern as proxy injection in spec 038)

### 2. `CLAWDEN_MANAGED` Environment Variable

Set `CLAWDEN_MANAGED=1` in the runtime's environment when launched via `clawden run` or `clawden up`. This gives **all** runtimes a universal signal that an outer orchestrator is managing security:

```rust
// In build_runtime_env_vars() — injected for every runtime
env.insert("CLAWDEN_MANAGED".to_string(), "1".to_string());
```

Runtimes that understand this variable can switch to a relaxed profile even without config file injection (fallback for runtimes that don't use `--config-dir`). This is especially important for OpenClaw and NanoClaw which use env-var-based configuration.

### 3. Resource Limit Propagation

For Docker mode, ensure container resource limits are set appropriately:

```rust
// When building docker run args
docker_args.push("--memory=4g".into());        // Generous default
docker_args.push("--ulimit=nofile=65536:65536".into());
docker_args.push("--security-opt=seccomp=unconfined".into());  // ClawDen sandbox handles this
```

For Direct mode, ClawDen can set `setrlimit()` before `exec()` if the user requests specific limits in `clawden.yaml`:

```yaml
runtimes:
  zeroclaw:
    security:
      memory_limit: "4g"    # Optional, default = no limit
      max_open_files: 65536  # Optional, default = system default
```

### 4. Sandbox Delegation

When `security.sandbox_tools = false` is injected into a runtime's config:

- **ZeroClaw**: skips its internal bwrap/namespace isolation for tool calls
- **OpenFang**: disables its Rust-native sandbox layer for tool execution
- **OpenClaw**: sets `OPENCLAW_WORKER_ISOLATION=none` to skip worker thread isolation
- ClawDen's `clawden-sandbox` wrapper remains active if the `sandbox` tool is enabled (spec 024)
- Tools execute with ClawDen's isolation boundary, not a nested one

This eliminates the double-sandbox conflict while maintaining isolation.

### 5. SecurityConfig Extension

Extend the existing `SecurityConfig` in `clawden-config`:

```rust
pub struct SecurityConfig {
    pub allowlist: Vec<String>,
    pub sandboxed: bool,
    // New fields:
    pub profile: Option<String>,          // "strict" | "managed" | "permissive"
    pub memory_limit: Option<String>,     // e.g., "4g", "unlimited"
    pub max_open_files: Option<u64>,      // e.g., 65536
    pub seccomp_enabled: Option<bool>,    // Override runtime default
    pub drop_capabilities: Option<bool>,  // Override runtime default
    pub delegate_sandbox: Option<bool>,   // true = use ClawDen sandbox
}
```

### 6. Config Translator Updates

**ZeroClawConfigTranslator** — emit `[security]` section in TOML output:
```rust
if let Some(ref sec) = config.security {
    if sec.profile.as_deref() == Some("managed") {
        native["security"]["profile"] = "managed".into();
        native["security"]["rlimit_as"] = 0.into();
        native["security"]["rlimit_nofile"] = 0.into();
        native["security"]["seccomp"] = "disabled".into();
        native["security"]["drop_capabilities"] = false.into();
        native["security"]["sandbox_tools"] = false.into();
    }
}
```

**OpenClawConfigTranslator** — emit security env vars:
```rust
if let Some(ref sec) = config.security {
    if sec.profile.as_deref() == Some("managed") {
        native["workerIsolation"] = "none".into();
        native["sandboxMode"] = "external".into();
    }
}
```

**OpenFang** (uses TOML via `generate_toml_config`) — emit `[security]` section with additional gRPC fields:
```rust
if let Some(ref sec) = config.security {
    if sec.profile.as_deref() == Some("managed") {
        // Same base fields as ZeroClaw, plus:
        native["security"]["tls_required"] = false.into();
        native["security"]["bind_address"] = "0.0.0.0".into();
    }
}
```

## Alternatives Considered

### A. Patch runtimes upstream to detect ClawDen
Rejected: Creates coupling dependencies. Runtimes shouldn't need to know about ClawDen.

### B. Only use Docker mode for isolation
Rejected: Direct mode is the default (spec 033) and many users prefer it. Can't require Docker.

### C. Override via environment variables only
Partially adopted (`CLAWDEN_MANAGED=1`), but insufficient alone — ZeroClaw's and OpenFang's config files take precedence over env vars for security settings (same class of issue as spec 038 proxy bug). For OpenClaw, env vars are the primary mechanism and are sufficient.

### D. Strip ZeroClaw's seccomp at the kernel level
Rejected: Requires root, fragile, and breaks the security model for standalone usage.

## Checklist

- [ ] Add `inject_security_profile()` to `generate_toml_config()` in `config_gen.rs` (ZeroClaw + OpenFang)
- [ ] Add `inject_security_env_vars()` for OpenClaw env-var-based security relaxation
- [ ] Set `security.profile = "managed"` with relaxed rlimits, seccomp, capabilities
- [ ] Set `CLAWDEN_MANAGED=1` env var in `build_runtime_env_vars()` for all runtimes
- [ ] Extend `SecurityConfig` struct with new fields (profile, memory_limit, max_open_files, etc.)
- [ ] Update `ZeroClawConfigTranslator::to_native()` to emit `[security]` section
- [ ] Update `OpenClawConfigTranslator::to_runtime_config()` to emit security fields
- [ ] Add OpenFang-specific security fields (tls_required, bind_address) to TOML injection
- [ ] Add Docker `--ulimit` and `--security-opt` flags for Docker mode launches
- [ ] Add optional `security:` section to `clawden.yaml` runtime config schema
- [ ] Verify multi-channel launch (Telegram + Discord) stays within FD limits
- [ ] Verify tool execution works without double-sandbox EPERM
- [ ] Verify health check probes succeed (no capability-drop interference)
- [ ] Verify OpenFang gRPC binding works in Docker mode with relaxed TLS
- [ ] Verify OpenClaw worker threads function under ClawDen sandbox
- [ ] All clawden-cli tests pass

## Notes

- This is the same class of config-override bug as spec 038 (proxy) and spec 031 (config injection): the runtime's own config silently overrides what ClawDen intends. The fix pattern is the same — inject correct values during config generation.
- The `CLAWDEN_MANAGED` env var can be reused by future specs as a universal "orchestrator present" signal for any runtime.
- NullClaw likely has the same issue (similar Rust codebase) — audit separately and apply the same pattern.
- PicoClaw and OpenFang should be audited but may not have strict security defaults.
