---
status: complete
created: 2026-03-05
priority: medium
tags:
- bugfix
- feishu
- channels
- direct-mode
- config
created_at: 2026-03-05T09:05:32.048231370Z
updated_at: 2026-03-05T09:05:39.325862306Z
completed_at: 2026-03-05T09:05:39.325862306Z
transitions:
- status: complete
  at: 2026-03-05T09:05:39.325862306Z
---

# Feishu/Lark Channel Not Configured in Direct Mode — Missing TOML Config Emitter

## Problem

Running `clawden run --channel feishu zeroclaw` with valid `FEISHU_APP_ID` and `FEISHU_APP_SECRET` environment variables results in the runtime reporting "No real-time channels configured; channel supervisor disabled". The Feishu channel is silently dropped during config generation.

## Root Cause

The TOML config generator in `crates/clawden-cli/src/commands/config_gen.rs` has a `match channel_type.as_str()` block that handles `telegram`, `discord`, `slack`, and `signal` — but has no arm for `"feishu" | "lark"`. The channel falls through to the `_ => {}` catch-all, producing an empty `row` that is discarded by the `if !row.is_empty()` guard. Consequently, the `[channels_config.feishu]` section is never written to `config.toml`.

### What works

- **Docker mode** (`clawden up`): Channel env vars (`ZEROCLAW_FEISHU_APP_ID`, etc.) are correctly generated via `ChannelCredentialMapper::zeroclaw_env_vars()` in `clawden-config`.
- **CLI env population** (`run.rs`): `FEISHU_APP_ID`/`FEISHU_APP_SECRET` are correctly read into `channel.extra["app_id"]` / `channel.extra["app_secret"]`.
- **All other config mappers**: `openclaw_channel_config`, `picoclaw_channel_config`, `nanoclaw_env_vars`, `ironclaw_channel_config`, `nullclaw_channel_config`, `microclaw_channel_config` all handle feishu.

### What fails

- **Direct-mode TOML generation** (`generate_toml_config` in `config_gen.rs`): Missing `"feishu" | "lark"` match arm means the populated `app_id`/`app_secret` in `channel.extra` are never serialized to TOML.

## Impact

- All direct-mode runs (`clawden run --channel feishu <runtime>`) silently ignore the Feishu channel for any runtime that reads from `config.toml` (ZeroClaw, OpenFang, etc.)
- Docker mode is unaffected

## Fix

- [x] Add `"feishu" | "lark"` match arm to `generate_toml_config()` in `config_gen.rs` that extracts `app_id` and `app_secret` from `channel.extra` and inserts them into the TOML row
- [x] Verify all other config paths (Docker env vars, OpenClaw JSON, PicoClaw, NanoClaw, IronClaw, NullClaw, MicroClaw) already handle feishu — confirmed
- [x] Verify `cargo test -p clawden-cli` passes

## Affected Files

- `crates/clawden-cli/src/commands/config_gen.rs` (fix applied)

## Related

- Spec 018 (channel-support-matrix): defines Feishu as supported for OpenClaw, PicoClaw, ZeroClaw, OpenFang
- Spec 031 (direct-mode-config-injection): TOML config generation framework
- Spec 029 (docker-mode-config-injection): Docker path (unaffected)

## Secondary Finding

The OpenFang adapter metadata (`crates/clawden-adapters/src/openfang.rs`) is missing Feishu (and ~10 other channels per spec 018) from its `channel_support` HashMap. This only affects proxy routing decisions, not config injection, and is out of scope for this fix.