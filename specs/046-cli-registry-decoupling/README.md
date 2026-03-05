---
status: complete
created: 2026-03-05
priority: high
tags:
- refactor
- cli
- channel
- provider
- registry
- architecture
created_at: 2026-03-05T09:05:53.557348178Z
updated_at: 2026-03-05T09:21:22.147031204Z
completed_at: 2026-03-05T09:21:22.147031204Z
transitions:
- status: complete
  at: 2026-03-05T09:21:22.147031204Z
---

# CLI Runtime & Channel Decoupling — Consume Core Registries Instead of Hardcoded Match Statements

## Overview

Specs 041 and 043 successfully centralized runtime metadata into `RuntimeDescriptor` and created `ProviderDescriptor` and `ChannelDescriptor` registries in `clawden-core`. The CLI now properly consumes `RuntimeDescriptor` for config format, health ports, and install logic.

However, the CLI **still contains 10+ hardcoded match statements** for channel and provider logic that duplicate or ignore the registries it should be consuming. Adding a new channel or provider still requires editing 3–4 CLI files despite the registries already existing.

### Current Hardcoding Hotspots

| File | What's Hardcoded | Registry That Should Drive It |
|------|-----------------|------------------------------|
| `config_gen.rs` L223–290 | Per-channel TOML field names (`bot_token`, `allowed_users`, `guild_id`, etc.) | `ChannelDescriptor.required_credentials` |
| `config_gen.rs` L270–291 | OpenClaw model prefixing (`openrouter/model` format) | Should be adapter-level or `RuntimeDescriptor` callback |
| `config_gen.rs` L330–340 | ZeroClaw `cli = true` required config default | Should be `RuntimeDescriptor.required_config_defaults` |
| `config_gen.rs` L440–465 | OpenClaw `CONFIG_PATH` env var | Should be `RuntimeDescriptor` metadata |
| `up.rs` L344–430 | Channel env var validation (per-channel match) | `ChannelDescriptor.token_env_var` + `required_credentials` |
| `up.rs` L599–639 | Runtime-prefixed env var expansion (`{RUNTIME}_LLM_PROVIDER`) | Should be a `clawden-core` utility |
| `up.rs` L955–965 | `infer_provider_type()` string → enum | `ProviderDescriptor.name` lookup |
| `run.rs` L150–196 | Per-channel env auto-population (Slack `app_token`, Feishu `app_id`) | `ChannelDescriptor.required_credentials` iteration |
| `run.rs` L528–542 | Telegram-only `allowed_users` check | `ChannelDescriptor` feature flag |
| `run.rs` L693–702 | `provider_env_key_aliases()` match | `ProviderDescriptor.env_vars` |

**Risk:** Adding a new channel requires 4 file edits (registry + 3 CLI files). Adding a provider requires 3 edits. The registries provide no single-source-of-truth benefit until the CLI actually consumes them.

### Why Now

- The registries already exist — this is wiring, not design work
- Spec 042 (OpenClaw Telegram config parity) exposed the cost of adding channel features via scattered matches
- Multiple channel additions are planned; each will hit the same 3–4 file edit pattern

### Depends On

- 041-runtime-descriptor-refactor (complete) — established the `RuntimeDescriptor` pattern
- 043-rust-codebase-structural-refactor (complete) — created `ProviderDescriptor` and `ChannelDescriptor` registries

## Design

### 1. Extend `ChannelDescriptor` with Config-Gen Metadata

Add fields to drive config generation and validation generically:

```rust
pub struct ChannelDescriptor {
    // existing
    pub channel_type: ChannelType,
    pub token_env_var: &'static str,
    pub required_credentials: &'static [&'static str],
    pub optional_credentials: &'static [&'static str],
    // new
    pub supports_allowed_users: bool,      // only telegram today
    pub extra_env_vars: &'static [(&'static str, &'static str)], // (field, ENV_NAME) pairs
}
```

### 2. Extend `RuntimeDescriptor` with Config Defaults & Env Metadata

```rust
pub struct RuntimeDescriptor {
    // existing fields...
    // new
    pub required_config_defaults: &'static [(&'static str, &'static str, &'static str)], // (section, key, value)
    pub extra_env_vars: &'static [(&'static str, &'static str)], // (ENV_NAME, description) — e.g. OPENCLAW_CONFIG_PATH
    pub model_transform: Option<fn(provider: &str, model: &str) -> String>, // e.g. OpenClaw's provider-prefix logic
}
```

### 3. Add Provider Lookup Helper to `ProviderDescriptor`

```rust
impl ProviderDescriptor {
    pub fn from_name(name: &str) -> Option<&'static ProviderDescriptor> { ... }
    pub fn env_var_names(&self) -> &[&str] { &self.env_vars }
}
```

### 4. Refactor CLI to Consume Registries

Replace each hardcoded match with a registry lookup:

- **`config_gen.rs` channel field generation** → iterate `ChannelDescriptor.required_credentials` and `optional_credentials` to build TOML/JSON fields
- **`config_gen.rs` ZeroClaw `cli=true`** → consume `RuntimeDescriptor.required_config_defaults`
- **`config_gen.rs` OpenClaw model prefixing** → delegate to `RuntimeDescriptor.model_transform`
- **`config_gen.rs` OpenClaw `CONFIG_PATH`** → consume `RuntimeDescriptor.extra_env_vars`
- **`up.rs` channel validation** → iterate `ChannelDescriptor` fields for env var checks
- **`up.rs` `infer_provider_type()`** → replace with `ProviderDescriptor::from_name()`
- **`up.rs` runtime env expansion** → extract to `clawden-core` utility
- **`run.rs` channel auto-population** → iterate `ChannelDescriptor.required_credentials`
- **`run.rs` `allowed_users` check** → check `ChannelDescriptor.supports_allowed_users`
- **`run.rs` `provider_env_key_aliases()`** → replace with `ProviderDescriptor.env_vars`

### 5. Delete Dead CLI Code

Remove the following once registry consumption is wired:
- `infer_provider_type()` in up.rs
- `provider_env_key_aliases()` in run.rs
- All per-channel match arms in `add_channel_requirements()`
- All per-channel match arms in `generate_toml_config()` field generation

## Plan

- [x] Extend `ChannelDescriptor` with `supports_allowed_users` and `extra_env_vars`
- [x] Extend `RuntimeDescriptor` with `required_config_defaults`, `extra_env_vars`, and `model_transform`
- [x] Add `ProviderDescriptor::from_name()` lookup helper
- [x] Refactor `config_gen.rs` — channel field generation via descriptor iteration
- [x] Refactor `config_gen.rs` — ZeroClaw defaults and OpenClaw model/env via descriptor
- [x] Refactor `up.rs` — channel validation via descriptor, remove `infer_provider_type()`
- [x] Refactor `up.rs` — runtime env expansion to core utility
- [x] Refactor `run.rs` — channel auto-population and allowed_users via descriptor
- [x] Refactor `run.rs` — provider env aliases via `ProviderDescriptor.env_vars`
- [x] Delete dead hardcoded match code from CLI
- [x] Validate no behavioral regression

## Test

- [x] `cargo test -p clawden-core --quiet` passes with descriptor extensions
- [x] `cargo test -p clawden-cli --quiet` passes with all refactors
- [x] `cargo clippy --workspace --quiet` clean
- [x] `cargo build --workspace --no-default-features --quiet` succeeds
- [x] Adding a new channel requires only 1 entry in `CHANNELS` array — verify no CLI edits needed
- [x] Adding a new provider requires only 1 entry in `PROVIDERS` array — verify no CLI edits needed
- [x] `clawden run`, `clawden up`, `clawden init` behaviors are preserved

## Notes

- This is the logical continuation of specs 041 (runtime descriptors) and 043 (provider/channel registries). Those specs built the registries; this spec wires them to the CLI.
- The `RuntimeDescriptor` is currently a static struct array — `model_transform` as `Option<fn(...)>` keeps it data-driven without requiring trait objects.
- Dashboard (React) channel display is out of scope — it has its own rendering logic.
- Docker entrypoint.sh channel/runtime matching is shell script and out of scope.
- `ChannelCredentialMapper` in `clawden-core` already exists and handles env→config mapping for some channels — this spec should consolidate with it, not duplicate.