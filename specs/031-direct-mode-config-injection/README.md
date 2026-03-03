---
status: in-progress
created: 2026-03-03
priority: high
tags:
- direct-mode
- config
- channels
- bug
- cli
depends_on:
- 013-config-management
- 029-docker-mode-config-injection
- 032-openfang-runtime-adapter
created_at: 2026-03-03T03:09:07.295096Z
updated_at: 2026-03-03T06:59:43.779696Z
transitions:
- status: in-progress
  at: 2026-03-03T06:59:43.779696Z
---
# Direct Mode Config Injection — Config-Dir Translation

## Overview

When `clawden up` runs in direct mode, it passes channel credentials and provider config as env vars to the spawned runtime process. However, runtimes like zeroclaw prioritize their own config file (`~/.zeroclaw/config.toml`) over env vars — so a stale `bot_token` in zeroclaw's config silently overrides the correct value from `clawden.yaml` + `.env`.

The fix: generate a translated runtime config file from `clawden.yaml` into a per-project config directory under `~/.clawden/configs/<project_hash>/<runtime>/`, then pass `--config-dir <path>` to the runtime. This makes clawden the single source of truth when launching via `clawden up`.

## Context

### Root Cause

- `clawden up` correctly resolves `$ENV_VAR` refs and passes `TELEGRAM_BOT_TOKEN` / `ZEROCLAW_TELEGRAM_BOT_TOKEN` as env vars to the spawned process
- zeroclaw loads `~/.zeroclaw/config.toml` at startup, which may contain a stale `[channels_config.telegram].bot_token`
- zeroclaw's config.toml takes precedence over env vars for channel credentials
- **Result**: user sees `401 Unauthorized` on a valid token because zeroclaw reads the wrong value

### Why `--config-dir` Solves This

Three runtimes (zeroclaw, picoclaw, nullclaw) already support `--config-dir` — listed in `runtime_supported_extra_args()`. This flag tells the runtime to read config from a custom directory instead of the default (`~/.zeroclaw/`, etc.). By generating a translated config file there, clawden controls exactly what the runtime sees.

### Runtime Config Format Map

| Runtime   | Language   | Config Format | Config File               | `--config-dir` | Status   |
| --------- | ---------- | ------------- | ------------------------- | -------------- | -------- |
| zeroclaw  | Rust       | TOML          | `~/.zeroclaw/config.toml` | ✅ Yes          | Phase 1  |
| picoclaw  | Go         | JSON          | config dir-based          | ✅ Yes          | Phase 1  |
| nullclaw  | —          | TOML          | config dir-based          | ✅ Yes          | Phase 1  |
| openclaw  | TypeScript | JSON5         | env vars only             | ❌ No           | Env-only |
| nanoclaw  | TypeScript | Code/inline   | env vars only             | ❌ No           | Env-only |
| ironclaw  | —          | WASM caps     | —                         | ❌ No           | Phase 2  |
| microclaw | —          | YAML-like     | —                         | ❌ No           | Phase 2  |
| openfang  | Rust       | TOML          | `~/.openfang/config.toml` | ❌ No           | Phase 2  |

### clawden.yaml → Runtime Config Field Mapping

#### ZeroClaw (TOML)

| clawden.yaml                      | config.toml field                          | Notes                                    |
| --------------------------------- | ------------------------------------------ | ---------------------------------------- |
| `provider`                        | `default_provider`                         | e.g. "openrouter"                        |
| `model`                           | `default_model`                            | e.g. "anthropic/claude-sonnet-4-6"       |
| `providers.<name>.api_key`        | `reliability.api_keys`                     | Array of `{provider, key}`               |
| `channels.telegram.token`         | `[channels_config.telegram].bot_token`     |                                          |
| `channels.telegram.allowed_users` | `[channels_config.telegram].allowed_users` |                                          |
| `channels.discord.token`          | `[channels_config.discord].bot_token`      |                                          |
| `channels.discord.guild`          | `[channels_config.discord].guild_id`       |                                          |
| `channels.slack.bot_token`        | `[channels_config.slack].bot_token`        |                                          |
| `channels.slack.app_token`        | `[channels_config.slack].app_token`        |                                          |
| `channels.signal.phone`           | `[channels_config.signal].phone`           |                                          |
| `channels.signal.token`           | `[channels_config.signal].token`           |                                          |
| `config.*`                        | Merged as-is into TOML root                | Catch-all for runtime-specific overrides |

#### PicoClaw (JSON)

| clawden.yaml               | JSON field               | Notes                           |
| -------------------------- | ------------------------ | ------------------------------- |
| `provider`                 | `llm.provider`           | PicoClaw uses "llm" not "model" |
| `model`                    | `llm.model`              |                                 |
| `providers.<name>.api_key` | `llm.apiKeyRef`          |                                 |
| `channels.<name>`          | Per-channel JSON objects | Via `picoclaw_channel_config()` |
| `config.*`                 | Merged into root         |                                 |

#### OpenClaw (JSON5) — env-only for now

No `--config-dir` support. Relies entirely on env vars + Docker `-e` flags. The `openclaw_channel_config()` function produces JSON channel objects for Docker mode config store but not for direct-mode file generation.

#### NanoClaw — env-only

No `--config-dir` support. Uses `NANOCLAW_*` prefixed env vars via `nanoclaw_env_vars()`.

### Related Specs

- **029-docker-mode-config-injection**: Fixed the same gap for Docker mode (via `-e` flags)
- **013-config-management**: Defines `RuntimeConfigTranslator` traits and canonical schema

## Design

### 1. Add `toml` Crate

Add `toml = "0.8"` to workspace dependencies and `clawden-cli`'s Cargo.toml.

### 2. Config Directory Layout

```
~/.clawden/configs/<project_hash>/
  └── <runtime>/
      └── config.toml     # (or config.json for picoclaw)
```

The `project_hash` isolates configs per-project directory (already used for pid-file scoping). The runtime subdirectory is what gets passed to `--config-dir`.

### 3. Config Generation Module

New file `crates/clawden-cli/src/commands/config_gen.rs` with per-runtime generators:

```rust
/// Generate a runtime config directory from ClawDenYaml.
/// Returns the path to pass as --config-dir, or None if the runtime
/// doesn't support config-dir injection.
pub fn generate_config_dir(
    config: &ClawDenYaml,
    runtime: &str,
    project_hash: &str,
) -> Result<Option<PathBuf>>
```

Dispatches to:
- `generate_zeroclaw_config()` → writes `config.toml`
- `generate_picoclaw_config()` → writes `config.json`
- `generate_nullclaw_config()` → writes `config.toml`
- Returns `None` for openclaw/nanoclaw (no `--config-dir` support)

### 4. ZeroClaw TOML Generation

Builds a `toml::Value::Table` with:

```toml
default_provider = "openrouter"
default_model = "anthropic/claude-sonnet-4-6"

[channels_config]
cli = true

[channels_config.telegram]
bot_token = "<resolved-token>"
allowed_users = ["@user1"]

# ... any config overrides from clawden.yaml config field
```

Uses `toml::to_string_pretty()` to serialize.

### 5. Inject `--config-dir` into start args

In the `ExecutionMode::Direct` branch of `exec_up()`:

```rust
let config_dir = if let Some(cfg) = config.as_ref() {
    generate_config_dir(cfg, &runtime, &project_hash()?)?
} else {
    None
};

let mut args = installed.start_args.clone();
if let Some(dir) = &config_dir {
    args.push("--config-dir".to_string());
    args.push(dir.to_string_lossy().to_string());
}
```

### 6. Keep Env Vars as Supplementary

Continue passing env vars (via `build_runtime_env_vars`) since:
- They provide `CLAWDEN_CHANNELS`, `CLAWDEN_TOOLS` (runtime-agnostic)
- They serve as fallback for fields not in config files
- Docker mode still depends on them exclusively
- Runtimes without `--config-dir` (openclaw, nanoclaw) depend on them exclusively

### 7. Cleanup on `clawden down`

Remove `~/.clawden/configs/<project_hash>/` when `clawden down` is run.

### 8. `clawden up` Startup Validation

After spawning each runtime, `clawden up` must verify it actually started successfully. Currently runtime processes are spawned and assumed healthy — a crashed or misconfigured runtime (wrong token, missing dependency, port conflict) is only noticed when the user inspects logs manually.

#### 8.1 Known Health Endpoints per Runtime

`runtime_health_url()` today only checks env overrides (`CLAWDEN_HEALTH_URL_*` / `CLAWDEN_HEALTH_PORT_*`). Add built-in defaults for runtimes with known gateway ports:

| Runtime   | Default Health URL              | Source                            |
| --------- | ------------------------------- | --------------------------------- |
| zeroclaw  | `http://127.0.0.1:42617/health` | HTTP Gateway                      |
| openclaw  | `http://127.0.0.1:18789/health` | WS Gateway                        |
| picoclaw  | `http://127.0.0.1:8080/health`  | HTTP Gateway                      |
| nullclaw  | `http://127.0.0.1:3000/health`  | HTTP Gateway                      |
| openfang  | `http://127.0.0.1:4200/health`  | Dashboard port                    |
| nanoclaw  | —                               | No HTTP gateway (Agent SDK)       |
| ironclaw  | —                               | WASM/webhooks, no standard health |
| microclaw | —                               | Web UI port TBD                   |
| mimiclaw  | —                               | Embedded firmware (serial/MQTT)   |

#### 8.2 Post-Start Readiness Check

After `start_direct_with_env_and_project()` returns a pid, perform a readiness check:

1. **Process alive check** — verify the pid is still running after a 500ms grace period. If the process exited, print the last N lines of the log file and bail with a clear error.
2. **Health probe** (if health URL is known) — poll the health endpoint up to 5 times with 1s intervals. Report `✓ <runtime> ready` on success or `⚠ <runtime> started (pid N) but health check not responding` as a warning (non-fatal — some runtimes take longer or run without HTTP).
3. **Early crash detection** — if the process exits within the first 2 seconds, capture stderr/log tail and report `✗ <runtime> crashed on startup` with the error output.

#### 8.3 Config Validation Before Start

Before spawning a runtime in direct mode, validate that:
- The runtime binary exists and is executable (already done by `ensure_installed_runtime`)
- Required channel tokens are non-empty (a blank `TELEGRAM_BOT_TOKEN=` causes silent failures)
- Required provider API key is present when a provider is configured

Print actionable errors like:
```
Error: channel 'telegram' is enabled but TELEGRAM_BOT_TOKEN is empty.
  → Set it in .env or run: clawden init --reconfigure
```

## Plan

- [x] Add `toml = "0.8"` to workspace deps and `clawden-cli` Cargo.toml
- [x] Create `config_gen.rs` module with `generate_config_dir()` dispatcher
- [x] Implement `generate_zeroclaw_config()` — TOML generation
- [x] Implement `generate_picoclaw_config()` — JSON generation (stretch)
- [x] Create config dir at `~/.clawden/configs/<project_hash>/<runtime>/`
- [x] Inject `--config-dir` into start args in `exec_up()` direct-mode branch
- [x] Apply the same pattern for `exec_run()` direct-mode branch
- [x] Clean up config dirs on `clawden down`
- [x] Add default health URLs to `runtime_health_url()` for zeroclaw, openclaw, picoclaw, nullclaw, openfang
- [x] Add post-start readiness check in `exec_up()`: process-alive check after 500ms grace
- [x] Add health probe polling (up to 5× at 1s intervals) for runtimes with known health endpoints
- [x] Add early crash detection — capture log tail if process exits within 2s
- [x] Add pre-start config validation: non-empty channel tokens, provider API key presence
- [x] Print actionable error messages for missing credentials before start
- [x] Add test: generated TOML matches expected zeroclaw config.toml format
- [x] Add test: `--config-dir` arg is injected for supported runtimes only
- [x] Add test: post-start readiness check detects crashed runtime
- [x] Add test: pre-start validation catches empty channel token

## Test

- [x] `clawden up` with telegram channel in clawden.yaml → generated config.toml contains correct `[channels_config.telegram].bot_token`
- [x] `clawden up` with openrouter provider → generated config.toml has `default_provider = "openrouter"` and API key
- [x] Stale `~/.zeroclaw/config.toml` does NOT interfere when `--config-dir` is used
- [x] `clawden down` removes the generated config directory
- [x] Runtimes without `--config-dir` (openclaw, nanoclaw) still work via env vars only
- [x] `config` overrides from clawden.yaml are merged into the generated config file
- [ ] `clawden up` with openfang runtime → config.toml is generated with correct TOML structure and provider/channel fields
- [x] `clawden up` detects a runtime that crashes immediately and prints the log tail with a clear error
- [x] `clawden up` reports `✓ <runtime> ready` when the health endpoint responds within the polling window
- [x] `clawden up` warns (non-fatal) when health endpoint is not responding but process is alive
- [x] `clawden up` with an empty `TELEGRAM_BOT_TOKEN=` in .env → prints actionable error before starting the runtime
- [x] `clawden up` with a provider configured but no API key → prints actionable error before starting the runtime
- [ ] `clawden up` with openfang in multi-runtime config alongside zeroclaw → both start and receive independent health checks
- [x] Works alongside env var passthrough (no regression for Docker mode)