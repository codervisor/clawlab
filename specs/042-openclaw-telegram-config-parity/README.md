---
status: planned
created: 2026-03-05
priority: medium
tags:
- telegram
- openclaw
- channels
- config
- parity
created_at: 2026-03-05T06:59:07.922540818Z
updated_at: 2026-03-05T06:59:07.922540818Z
---

# OpenClaw Telegram Channel Parity — Full Config Translation for DM Policy, Groups & Access Control

## Overview

ClawDen's OpenClaw Telegram channel mapping currently translates only `token` and `allowFrom`. The [upstream OpenClaw Telegram docs](https://docs.openclaw.ai/channels/telegram) expose 30+ config fields governing DM policy, group access control, mention gating, streaming, webhooks, proxy, and more. Users who configure these features in ClawDen today get silent no-ops — the fields land in the `extra` catch-all but are never forwarded to OpenClaw's config.

This spec closes the gap between ClawDen's `clawden.yaml` channel schema and the full OpenClaw Telegram config surface, prioritized by user impact.

## Motivation

- Users coming from raw OpenClaw expect `dmPolicy`, `groupPolicy`, `groups`, and `requireMention` to work — these are in the Quick Setup on the official docs.
- `group_mode` already exists in `ChannelInstanceYaml` but is never mapped to OpenClaw's `groupPolicy`.
- The `extra` HashMap catches unknown keys but the `openclaw_channel_config()` and `openclaw_env_vars()` functions ignore them.
- Dashboard `ChannelOverview.tsx` only shows `allowed_users` and `group_mode` as optional fields for Telegram — minimal compared to what users actually need.

## Design

### Tier 1 — Access Control (must-have)

These fields are essential for any production Telegram deployment:

| `clawden.yaml` field | OpenClaw config target | Notes |
|---|---|---|
| `dm_policy` | `channels.telegram.dmPolicy` | `pairing` (default) / `allowlist` / `open` / `disabled` |
| `group_policy` | `channels.telegram.groupPolicy` | `open` / `allowlist` / `disabled` — also accept existing `group_mode` as alias |
| `group_allow_from` | `channels.telegram.groupAllowFrom` | Numeric Telegram user IDs for group sender allowlist |
| `groups` | `channels.telegram.groups` | Per-group config map (keys are group IDs or `*`), each with `requireMention`, `allowFrom`, `groupPolicy`, `skills`, `systemPrompt`, `enabled` |
| `default_to` | `channels.telegram.defaultTo` | Default Telegram target for CLI `--deliver` |

### Tier 2 — Delivery & Networking (high-value)

| `clawden.yaml` field | OpenClaw config target | Notes |
|---|---|---|
| `proxy` | `channels.telegram.proxy` | SOCKS/HTTP proxy URL for Bot API calls |
| `webhook_url` | `channels.telegram.webhookUrl` | Enables webhook mode |
| `webhook_secret` | `channels.telegram.webhookSecret` | Required when `webhook_url` is set |
| `webhook_port` | `channels.telegram.webhookPort` | Default 8787 |
| `streaming` | `channels.telegram.streaming` | `off` / `partial` / `block` / `progress` |
| `reply_to_mode` | `channels.telegram.replyToMode` | `off` / `first` / `all` |

### Tier 3 — Fine-tuning (pass-through via `extra`)

All remaining fields (`textChunkLimit`, `chunkMode`, `linkPreview`, `mediaMaxMb`, `capabilities.inlineButtons`, `actions.*`, `reactionNotifications`, `reactionLevel`, `commands.*`, `tokenFile`, `network.*`, `retry`) should be forwarded from the `extra` HashMap into the OpenClaw config fragment as-is. This avoids adding 20+ dedicated struct fields for rarely-configured options while still allowing power users to set them.

### Implementation Approach

1. **`ChannelInstanceYaml`** (`clawden-config/src/lib.rs`): Add Tier 1 and Tier 2 dedicated fields with `#[serde(default)]`. Keep `extra` for Tier 3.

2. **`openclaw_channel_config()`** (`clawden-config/src/lib.rs`): Extend the `"telegram"` match arm to emit all Tier 1 and Tier 2 fields into the JSON config fragment. Merge `extra` keys under `telegram.*` for Tier 3 pass-through.

3. **`openclaw_env_vars()`** (`clawden-config/src/lib.rs`): Forward `dm_policy` and `group_policy` as `OPENCLAW_TELEGRAM_DM_POLICY` and `OPENCLAW_TELEGRAM_GROUP_POLICY` for env-based injection mode.

4. **`up.rs` credential table** (`clawden-cli/src/commands/up.rs`): No changes needed — token resolution already works. Consider adding a note when `dm_policy: allowlist` is set but `allowed_users` is empty (OpenClaw rejects this).

5. **Dashboard** (`dashboard/src/components/channels/ChannelOverview.tsx`): Update `CHANNEL_FIELDS.telegram.optional` to include `dm_policy`, `group_policy`, `groups`, `proxy`, `webhook_url`.

6. **`group_mode` backward compatibility**: Accept `group_mode` as an alias for `group_policy`. If both are set, `group_policy` wins with a warning.

## Plan

- [ ] Add Tier 1 fields (`dm_policy`, `group_policy`, `group_allow_from`, `groups`, `default_to`) to `ChannelInstanceYaml`
- [ ] Extend `openclaw_channel_config()` to emit Tier 1 fields into the Telegram JSON config
- [ ] Map `group_mode` → `groupPolicy` with backward compat
- [ ] Add Tier 2 fields (`proxy`, `webhook_url`, `webhook_secret`, `webhook_port`, `streaming`, `reply_to_mode`) to `ChannelInstanceYaml`
- [ ] Extend `openclaw_channel_config()` to emit Tier 2 fields
- [ ] Implement Tier 3 `extra` pass-through — merge remaining extra keys into the Telegram config fragment
- [ ] Extend `openclaw_env_vars()` to forward `dm_policy` and `group_policy`
- [ ] Update dashboard `CHANNEL_FIELDS.telegram` optional list
- [ ] Add config validation: warn when `dm_policy: allowlist` + empty `allowed_users`
- [ ] Add unit tests for new config translation paths

## Test

- [ ] `dm_policy: pairing` in `clawden.yaml` → OpenClaw config contains `telegram.dmPolicy: "pairing"`
- [ ] `group_policy: allowlist` + `group_allow_from: ["123"]` → correctly mapped
- [ ] `groups: { "*": { requireMention: true } }` → forwarded verbatim to OpenClaw config
- [ ] `group_mode: open` (legacy) → translated to `groupPolicy: "open"`
- [ ] `group_mode` + `group_policy` both set → `group_policy` wins, warning emitted
- [ ] `proxy: socks5://...` → appears in config fragment
- [ ] `webhook_url` + `webhook_secret` → both appear in config
- [ ] Extra keys (e.g., `textChunkLimit: 3000`) → forwarded under `telegram.*`
- [ ] `dm_policy: allowlist` with empty `allowed_users` → validation warning
- [ ] Dashboard shows new optional fields for Telegram channel type
- [ ] Existing configs with only `token` + `allowed_users` continue to work unchanged

## Notes

- This spec is scoped to OpenClaw only. ZeroClaw, NanoClaw, and PicoClaw have simpler Telegram integrations with fewer config knobs — those runtimes should get similar parity specs as their docs mature.
- Multi-account support (`channels.telegram.accounts.*`, `defaultAccount`) is deferred — it requires schema changes beyond per-channel-instance config.
- The `extra` pass-through approach mirrors how `ChannelInstanceYaml` already uses `#[serde(flatten)]` — we just need to actually forward those values in the config mapper.
- OpenClaw's pairing flow (`openclaw pairing list/approve`) is runtime-side; ClawDen only needs to set `dmPolicy: pairing` and let OpenClaw handle the rest.
