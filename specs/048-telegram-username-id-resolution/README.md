---
status: in-progress
created: 2026-03-05
priority: high
tags:
- telegram
- openclaw
- identity
- username
- config
- ux
created_at: 2026-03-05T14:08:24.530411Z
updated_at: 2026-03-05T14:27:18.265681Z
transitions:
- status: in-progress
  at: 2026-03-05T14:27:18.265681Z
---
# Telegram Username-to-ID Resolution for OpenClaw

## Overview

OpenClaw's Telegram `allowFrom` requires numeric user IDs, but users naturally want to specify `@username` (like ZeroClaw supports). Since Telegram's Bot API cannot resolve `@username` → numeric ID for private users without prior interaction, ClawDen needs a smart resolution layer that bridges the gap transparently.

This spec adds username-to-ID resolution so users can write `allowed_users: ["marvzhang"]` in `clawden.yaml` and ClawDen handles the translation before passing config to OpenClaw.

## Motivation

- ZeroClaw accepts both usernames and numeric IDs in `allowed_users` because it checks both `from.username` and `from.id` at runtime. OpenClaw only consumes `allowFrom` with numeric IDs.
- Users switching between runtimes or coming from ZeroClaw expect usernames to "just work."
- Requiring users to manually look up their numeric Telegram ID is a friction point — most users know their `@username` but not their numeric ID.
- The Telegram Bot API `getUpdates` response includes both `from.username` and `from.id`, so resolution is possible given one prior interaction.

## Design

### Detection: Numeric vs Username

A value in `allowed_users` is numeric if it matches `^[0-9]+$`. Everything else is treated as a username (with `@` prefix stripped). This mirrors ZeroClaw's `normalize_identity()`.

### Resolution Strategy (3-phase)

**Phase 1 — Cache lookup**
Check `.clawden/telegram-ids.json` for a previously resolved mapping. If found, use cached numeric ID immediately. Cache entries include a timestamp for optional staleness warnings.

```json
{
  "marvzhang": { "id": 123456789, "resolved_at": "2026-03-05T10:00:00Z" }
}
```

**Phase 2 — History scan**
Call `getUpdates` (non-destructive, no offset commit) and scan `from.username` + `from.id` across all recent messages. If a matching username is found, resolve it, cache the mapping, and proceed.

**Phase 3 — Interactive polling**
If Phase 2 finds no match, print a prompt:
```
⚠ Cannot resolve Telegram username "marvzhang" to numeric ID.
  → Send any message to your bot from @marvzhang, then press Enter...
```
Poll `getUpdates` in a short loop (30s timeout, 2s intervals). On match, resolve, cache, and continue. On timeout, exit with actionable error.

### Integration Points

#### `clawden run` / `clawden up` (config generation)

Before generating OpenClaw config, resolve all non-numeric `allowed_users` entries:

1. Partition `allowed_users` into numeric IDs (pass through) and usernames (resolve).
2. Run 3-phase resolution for each username.
3. Substitute resolved IDs into the `allowFrom` config passed to OpenClaw.
4. Log resolved mappings: `Telegram: resolved @marvzhang → 123456789`

#### `clawden telegram resolve-id` (explicit command)

New subcommand for manual resolution:
```bash
clawden telegram resolve-id marvzhang
# Resolves and prints: marvzhang → 123456789
# Also caches the result
```
Useful for scripting, CI, or pre-populating the cache.

#### Cache file: `.clawden/telegram-ids.json`

- Created automatically on first resolution.
- Gitignored (add to default `.gitignore` template).
- Entries are additive — new resolutions merge, never overwrite unless ID changes.
- If a username resolves to a different ID (user changed account), warn and update.

### Edge Cases

- **User has no username**: Some Telegram users don't set a username. These users must use numeric ID directly — no resolution possible.
- **Multiple bots**: Cache is per-bot-token. Key the cache by bot token hash to avoid cross-bot collisions: `.clawden/telegram-ids-{token_hash_prefix}.json`.
- **`*` wildcard**: Pass through unchanged — never attempt resolution.
- **Already-resolved values**: If `allowed_users` contains numeric IDs, skip resolution entirely.
- **Offline / no network**: Fall back to cache. If cache misses, error with clear message.
- **getUpdates consumed by running instance**: If another process (e.g., OpenClaw itself) is polling the same bot, `getUpdates` returns empty. Detect this (`count=0` + running process check) and advise stopping the other poller first.

### Security Considerations

- Resolution happens locally in ClawDen CLI, not in OpenClaw runtime.
- Bot token is never logged or stored beyond existing config.
- Cache file contains only username → numeric ID mappings (public Telegram info).
- No temporary `allowFrom: ["*"]` — resolution completes before OpenClaw starts.

## Plan

- [x] Add `is_numeric_telegram_id()` helper to `clawden-config`
- [x] Implement `TelegramIdResolver` in `clawden-cli` with 3-phase resolution (cache → history → interactive poll)
- [x] Create `.clawden/telegram-ids.json` cache read/write logic
- [x] Integrate resolver into `run.rs` config generation path — resolve before `openclaw_channel_config()`
- [x] Integrate resolver into `up.rs` config generation path
- [x] Add `clawden telegram resolve-id <username>` subcommand
- [x] Add `.clawden/` to default `.gitignore` template
- [x] Log resolved mappings during run/up
- [ ] Handle edge cases: wildcard, no-username users, stale cache, cross-bot isolation
- [x] Add unit tests for resolution logic and cache operations
- [x] Add integration test: username in config → numeric ID in generated OpenClaw config

## Test

- [x] `allowed_users: ["marvzhang"]` with cached ID → OpenClaw config gets `allowFrom: ["123456789"]`
- [x] `allowed_users: ["123456789"]` → passed through unchanged (no resolution attempt)
- [ ] `allowed_users: ["*"]` → passed through unchanged
- [ ] `allowed_users: ["marvzhang", "123456789"]` → mixed resolution: username resolved, numeric passed through
- [ ] Cache miss + `getUpdates` has matching username → resolves and caches
- [ ] Cache miss + `getUpdates` empty + interactive prompt → polls and resolves on message
- [ ] Cache miss + timeout → clear error message with instructions
- [ ] Cache hit with different ID → warns and updates cache
- [x] `clawden telegram resolve-id marvzhang` → prints resolved ID and updates cache
- [x] Multiple bot tokens → separate cache files, no cross-contamination
- [x] ZeroClaw `allowed_users` with usernames → no resolution attempted (ZeroClaw handles natively)

## Notes

- Resolution is OpenClaw-specific. ZeroClaw, PicoClaw, NanoClaw handle username matching at runtime and don't need this layer.
- The 3-phase approach mirrors how ZeroClaw's pairing flow works conceptually — learn identity from actual Telegram interaction — but shifts it to config-generation time.
- Future enhancement: if OpenClaw adds native username support upstream, this resolver becomes a no-op optimization (cache speeds up startup, but resolution is no longer required).
- The `getUpdates` call in Phase 2 does NOT commit the offset (no `offset` param), so it won't consume updates that OpenClaw needs later.