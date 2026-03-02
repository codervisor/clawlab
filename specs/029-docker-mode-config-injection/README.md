---
status: in-progress
created: 2026-03-02
priority: high
tags:
- docker
- config
- channels
- bug
- adapter
depends_on:
- 017-docker-runtime-images
- 013-config-management
created_at: 2026-03-02T08:27:30.088282380Z
updated_at: 2026-03-02T09:06:43.741656288Z
transitions:
- status: in-progress
  at: 2026-03-02T09:04:49.604493321Z
---
# Docker Mode Config Injection — Channel & Env Passthrough

## Overview

`clawden up` in Docker mode silently drops all `clawden.yaml` configuration — channels, provider keys, model settings, and tool selections are never passed to the container. The runtime boots with defaults, resulting in "No real-time channels configured" even when the user has explicitly configured channels.

Direct mode works correctly: it calls `build_runtime_env_vars()` and `channels_for_runtime()`, then passes env vars and `--channels=` flags to the spawned process. Docker mode skips both entirely.

## Context

### Root Cause

In `up.rs`, the `ExecutionMode::Docker` branch:

```rust
ExecutionMode::Docker => {
    let rt = parse_runtime(&runtime)?;
    let record = manager.register_agent(...);
    manager.start_agent(&record.id).await?;
    // No config extraction, no env vars, no channel flags
}
```

Meanwhile the `ExecutionMode::Direct` branch correctly calls:
- `build_runtime_env_vars(cfg, &runtime)` — maps channel tokens, provider API keys, model config to env vars
- `channels_for_runtime(cfg, &runtime)` — extracts channel list for `--channels=` flag

Additionally, `ZeroClawAdapter::start()` (and other adapters) are stubs that return an `AgentHandle` without actually configuring the Docker container. `set_config()` is also a no-op.

### Impact

Any user running `clawden up` in Docker mode with channels configured in `clawden.yaml` gets a broken experience — the runtime starts but ignores all channel, provider, and model configuration.

### Related Specs

- **017-docker-runtime-images**: Defines the design for config passthrough (env vars per runtime) but implementation is incomplete for the Docker code path
- **013-config-management**: Defines config translator traits (`to_runtime_config`) — not wired into Docker mode
- **027-docker-compose-ux**: Improved CLI UX (log streaming, `down`, etc.) but only for Direct mode

## Design

### 1. Unify Config Extraction in `up.rs`

Move `build_runtime_env_vars()` and `channels_for_runtime()` calls **before** the `match mode` branch so both Docker and Direct modes have access to the translated config:

```rust
let env_vars = if let Some(cfg) = config.as_ref() {
    build_runtime_env_vars(cfg, &runtime)?
} else {
    Vec::new()
};

let channels = if let Some(cfg) = config.as_ref() {
    channels_for_runtime(cfg, &runtime)
} else {
    Vec::new()
};

match mode {
    ExecutionMode::Docker => {
        // Pass env_vars and channels to LifecycleManager / adapter
    }
    ExecutionMode::Direct | ExecutionMode::Auto => {
        // Existing direct-mode logic (already works)
    }
}
```

### 2. Extend `AgentConfig` with Env Vars and Channels

`AgentConfig` (in `clawden-core`) needs fields to carry the translated configuration:

```rust
pub struct AgentConfig {
    pub name: String,
    pub runtime: ClawRuntime,
    pub model: Option<String>,
    pub env_vars: Vec<(String, String)>,  // NEW
    pub channels: Vec<String>,            // NEW
    pub tools: Vec<String>,               // NEW
}
```

### 3. Wire Env Vars into Docker Adapter `start()`

Each adapter's `start()` receives `AgentConfig` which now carries env vars. For Docker mode, the adapter (or `LifecycleManager`) must pass these as `-e` flags to `docker run` or as environment entries in the compose config.

The adapter `start()` implementations should:
- Map `env_vars` to container environment variables
- Map `channels` to the runtime's `--channels=` argument or equivalent
- Map `tools` to the container's `TOOLS` env var (used by `entrypoint.sh`)

### 4. Config Passthrough for `run` Command

Apply the same fix to `clawden run` Docker mode — it has the same gap.

## Plan

- [x] Hoist `build_runtime_env_vars()` and `channels_for_runtime()` above the mode branch in `up.rs`
- [x] Extend `AgentConfig` with `env_vars`, `channels`, and `tools` fields
- [x] Pass extracted config into `LifecycleManager::register_agent()` / `start_agent()`
- [x] Update `ZeroClawAdapter::start()` to forward env vars and channels to the container
- [x] Update other Phase 1 adapters (OpenClaw, PicoClaw, NanoClaw) similarly
- [x] Apply the same config injection fix to `clawden run` Docker mode path
- [x] Add integration test: `clawden up` in Docker mode with telegram channel configured → runtime receives `TELEGRAM_BOT_TOKEN` env var

## Test

- [x] `clawden up` in Docker mode with `channels.telegram.token` in `clawden.yaml` → runtime starts with Telegram channel active (not "No real-time channels configured")
- [x] `clawden up` in Docker mode with `provider` and `model` in `clawden.yaml` → runtime receives provider API key and model env vars
- [x] `clawden up` in Docker mode with `tools: [git, http]` → container's `TOOLS` env var is set to `git,http`
- [x] `clawden run zeroclaw` in Docker mode passes channel and provider config to the container
- [x] Direct mode behavior is unchanged (no regression)
- [x] Missing env var references (e.g. `$UNSET_VAR`) produce a clear error at startup, not a silent empty value