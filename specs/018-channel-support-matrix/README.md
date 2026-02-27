---
status: planned
created: 2026-02-26
priority: high
tags:
- channels
- messaging
- integration
- telegram
- discord
- whatsapp
depends_on:
- 010-claw-runtime-interface
parent: 009-orchestration-platform
created_at: 2026-02-26T02:50:41.154596674Z
updated_at: 2026-02-27T12:58:54.851774Z
---
# Chat Channel Support Matrix & Unified Channel Layer

## Overview

Every claw runtime has independently built its own messaging integrations — Telegram bots, Discord adapters, WhatsApp bridges, etc. The result is fragmented: each uses different libraries, auth flows, and configuration patterns.

ClawDen makes this invisible to users. You list your channels in `clawden.yaml`, assign them to runtimes, and ClawDen handles the rest — credential injection and proxying for unsupported combos.

**Phase 1 runtimes**: OpenClaw (highest priority — most popular runtime), ZeroClaw, NanoClaw, and PicoClaw. IronClaw, NullClaw, and MicroClaw are deferred to Phase 2.

**Channel tiers** (within Phase 1 runtimes):
- **Tier 1**: Telegram, Discord, Feishu/Lark — high native coverage across Phase 1 runtimes, proves the config translation pipeline. Feishu/Lark is included because early adopters are expected to skew heavily toward the Chinese market.
- **Tier 2**: Slack, WhatsApp — full Phase 1 runtime coverage but more complex auth (Slack needs dual tokens; WhatsApp has divergent implementations per runtime).
- **Tier 3**: Signal, DingTalk, QQ, LINE, Matrix — partial coverage, requires channel proxy for most Phase 1 runtimes.

**Canonical runtime list**: Per [ClawCharts.com](https://clawcharts.com/) (February 2026): OpenClaw, Nanobot, PicoClaw, ZeroClaw, NanoClaw, IronClaw, TinyClaw, OpenFang.

## Channel Support Matrix

Data sourced from official GitHub repos (February 2026). Runtime list per [ClawCharts.com](https://clawcharts.com/).

### By Runtime

| Channel         |    OpenClaw    |  Nanobot   | PicoClaw |   ZeroClaw   |  NanoClaw   |   IronClaw   | TinyClaw |   OpenFang    |
| --------------- | :------------: | :--------: | :------: | :----------: | :---------: | :----------: | :------: | :-----------: |
| **Telegram**    |       ✅        |     ✅      |    ✅     |      ✅       |  ✅ (skill)  |   ✅ (WASM)   |    ✅     |       ✅       |
| **Discord**     |       ✅        |     ✅      |    ✅     |      ✅       |  ✅ (skill)  |   ✅ (WASM)   |    ✅     |       ✅       |
| **Slack**       |       ✅        |     ✅      |    ✅     |      ✅       |  ✅ (skill)  |   ✅ (WASM)   |    —     |       ✅       |
| **WhatsApp**    |  ✅ (Baileys)   | ✅ (bridge) |    ✅     | ✅ (Meta API) | ✅ (default) |      —       |  ✅ (QR)  | ✅ (Cloud API) |
| **Signal**      | ✅ (signal-cli) |     —      |    —     |      ✅       |      —      | ✅ (built-in) |    —     |       ✅       |
| **Matrix**      |       —        |     ✅      |    —     |      ✅       |      —      |      —       |    —     |       ✅       |
| **Email**       |       —        |     ✅      |    —     |      ✅       |      —      |      —       |    —     |       ✅       |
| **Feishu/Lark** |       ✅        |     ✅      |    ✅     |      ✅       |      —      |      —       |    —     |       ✅       |
| **DingTalk**    |       —        |     ✅      |    ✅     |      —       |      —      |      —       |    —     |       ✅       |
| **Mattermost**  |       ✅        |     —      |    —     |      ✅       |      —      |      —       |    —     |       ✅       |
| **IRC**         |       ✅        |     —      |    —     |      ✅       |      —      |      —       |    —     |       ✅       |
| **MS Teams**    |       ✅        |     —      |    —     |      —       |      —      |      —       |    —     |       ✅       |
| **iMessage**    |       ✅        |     —      |    —     |      ✅       |      —      |      —       |    —     |       —       |
| **Google Chat** |       ✅        |     —      |    —     |      —       |      —      |      —       |    —     |       ✅       |
| **QQ**          |       —        |     ✅      |    ✅     |      —       |      —      |      —       |    —     |       —       |
| **LINE**        |       —        |     —      |    ✅     |      —       |      —      |      —       |    —     |       ✅       |
| **Nostr**       |       ✅        |     —      |    —     |      ✅       |      —      |      —       |    —     |       ✅       |
| **Total**       |    **10+**     |   **10**   | **~10**  |   **16+**    |    **4**    |    **5**     |  **3**   |    **40**     |

### Phase 1 Runtimes — Channel Coverage

These are the priority runtimes for initial channel support:

| Channel         |    OpenClaw    |   ZeroClaw   |  NanoClaw   | PicoClaw |
| --------------- | :------------: | :----------: | :---------: | :------: |
| **Telegram**    |       ✅        |      ✅       |  ✅ (skill)  |    ✅     |
| **Discord**     |       ✅        |      ✅       |  ✅ (skill)  |    ✅     |
| **Slack**       |       ✅        |      ✅       |  ✅ (skill)  |    ✅     |
| **WhatsApp**    |  ✅ (Baileys)   | ✅ (Meta API) | ✅ (default) |    ✅     |
| **Signal**      | ✅ (signal-cli) |      ✅       |      —      |    —     |
| **Feishu/Lark** |       ✅        |      ✅       |      —      |    ✅     |

**Tier 1 channels** (Telegram, Discord, Feishu/Lark) have broad native coverage — these ship first to validate the config translation pipeline. **Tier 2 channels** (Slack, WhatsApp) have full runtime coverage but add auth complexity. **Tier 3 channels** (Signal, DingTalk, QQ, etc.) have partial coverage and require the channel proxy. For any channel a runtime doesn't natively support, ClawDen's proxy bridges the gap.

### Implementation Libraries by Runtime

| Runtime  | Telegram     | Discord      | WhatsApp         | Slack              |
| -------- | ------------ | ------------ | ---------------- | ------------------ |
| OpenClaw | grammY (JS)  | discord.js   | Baileys          | Bolt (JS)          |
| ZeroClaw | native Rust  | native Rust  | Meta Cloud API   | —                  |
| NanoClaw | skill-based  | skill-based  | default          | skill-based        |
| PicoClaw | native Go    | native Go    | —                | native Go          |
| IronClaw | WASM channel | WASM channel | —                | WASM tool          |
| OpenFang | native Rust  | native Rust  | Cloud API (Rust) | Socket Mode (Rust) |

## Design

### User-Facing: Channels in `clawden.yaml`

Channel config lives in `clawden.yaml` (see spec 017). Users define **named channel instances**, each with its own credentials, then assign them to runtimes by name. Each channel instance maps to exactly one runtime — a 1:1 relationship.

#### Channel Instance Naming

Channel keys are **instance names**, not channel types. The `type` field declares the platform:

```yaml
channels:
  support-tg:
    type: telegram
    token: $SUPPORT_TG_TOKEN
  creative-tg:
    type: telegram
    token: $CREATIVE_TG_TOKEN
  team-discord:
    type: discord
    token: $DISCORD_BOT_TOKEN

runtimes:
  - name: zeroclaw
    channels: [support-tg, team-discord]
    tools: [git]

  - name: picoclaw
    channels: [creative-tg]
    tools: [git]
```

Two separate Telegram bots, each wired to a different runtime. No conflicts.

**Shorthand**: When the instance name matches a known channel type and there's only one instance of that type, the `type` field can be omitted:

```yaml
channels:
  telegram:                     # name "telegram" → type inferred as telegram
    token: $TELEGRAM_BOT_TOKEN
  discord:                      # name "discord" → type inferred as discord
    token: $DISCORD_BOT_TOKEN
```

This keeps the simple case simple while supporting the multi-agent case cleanly.

#### Minimal Example

```yaml
runtime: zeroclaw
channels:
  telegram:
    token: $TELEGRAM_BOT_TOKEN
```

Three lines. Running agent on Telegram.

#### Common Fields (All Channel Instances)

| Field  | Required | Description                                                                                                                                                          |
| ------ | -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `type` | No*      | Channel platform. Inferred from instance name if it matches a known type. Required when instance name differs from type (e.g., `support-tg` needs `type: telegram`). |

*Known types: `telegram`, `discord`, `slack`, `whatsapp`, `signal`, `matrix`, `email`, `feishu`, `dingtalk`, `mattermost`, `irc`, `teams`, `imessage`, `google_chat`, `qq`, `line`, `nostr`.

#### Per-Type Fields

| Type     | Required Fields          | Optional Fields                            |
| -------- | ------------------------ | ------------------------------------------ |
| telegram | `token`                  | `allowed_users`, `group_mode`              |
| discord  | `token`                  | `guild`, `allowed_roles`                   |
| slack    | `bot_token`, `app_token` | `allowed_channels`                         |
| whatsapp | `token`                  | (implementation auto-selected per runtime) |
| signal   | `phone`                  | `signal_cli_path`                          |

Most channels: just `type` (if needed) + `token`. Done.

#### Secrets Handling

Tokens use `$ENV_VAR` references — never stored as plaintext:

```yaml
channels:
  telegram:
    token: $TELEGRAM_BOT_TOKEN  # resolved from env or .env file
```

ClawDen auto-loads `.env` next to `clawden.yaml`.

### What ClawDen Does Internally

User writes simple YAML. ClawDen resolves instance names → types → per-runtime config:

```
clawden.yaml                     What each runtime actually needs
────────────                     ──────────────────────────────
channels:                        ZeroClaw: ZEROCLAW_TELEGRAM_BOT_TOKEN env var
  support-tg:                            (from support-tg instance)
    type: telegram               PicoClaw: config.telegram.token in JSON
    token: $SUPPORT_TOKEN                (from creative-tg instance)
  creative-tg:
    type: telegram
    token: $CREATIVE_TOKEN

runtimes:
  - name: zeroclaw
    channels: [support-tg]
  - name: picoclaw
    channels: [creative-tg]
```

Each CRI adapter maps from instance type + credentials → runtime-specific format. Users never see this.

### Config Translation by Runtime

This table shows how ClawDen translates each channel instance into the format each runtime expects. This is the internal mapping that CRI adapters implement.

#### OpenClaw (JSON5 config — highest priority)

OpenClaw uses JSON5 config files with per-channel library configuration:

| Channel  | Library    | Config translation                                                                    |
| -------- | ---------- | ------------------------------------------------------------------------------------- |
| Telegram | grammY     | `{ "telegram": { "token": "<resolved>" } }` in JSON5 config                           |
| Discord  | discord.js | `{ "discord": { "token": "<resolved>", "guild": "..." } }` in JSON5 config            |
| Slack    | Bolt       | `{ "slack": { "botToken": "<resolved>", "appToken": "<resolved>" } }` in JSON5 config |
| WhatsApp | Baileys    | `{ "whatsapp": { "token": "<resolved>" } }` in JSON5 config                           |

OpenClaw's gateway architecture (port 18789) means channel tokens go into its config file, not env vars. ClawDen generates the JSON5 config and mounts it.

#### ZeroClaw (env vars + TOML config)

ZeroClaw uses environment variables prefixed with `ZEROCLAW_`:

| Channel  | Env var(s)                    | Notes                      |
| -------- | ----------------------------- | -------------------------- |
| Telegram | `ZEROCLAW_TELEGRAM_BOT_TOKEN` | Native Rust implementation |
| Discord  | `ZEROCLAW_DISCORD_BOT_TOKEN`  | Native Rust implementation |
| WhatsApp | `ZEROCLAW_WHATSAPP_TOKEN`     | Meta Cloud API             |
| Signal   | `ZEROCLAW_SIGNAL_PHONE`       | Native Rust, signal-cli    |

ClawDen sets these env vars when spawning the ZeroClaw process. TOML config file is also supported (`config.toml`) but env vars take precedence.

#### NanoClaw (code-driven, skill-based)

NanoClaw uses skill-based channel registration via the Claude Agent SDK:

| Channel  | Method           | Config translation                                                      |
| -------- | ---------------- | ----------------------------------------------------------------------- |
| Telegram | Skill injection  | Pass token via `NANOCLAW_TELEGRAM_TOKEN` env var; skill auto-registers  |
| Discord  | Skill injection  | Pass token via `NANOCLAW_DISCORD_TOKEN` env var; skill auto-registers   |
| Slack    | Skill injection  | Pass tokens via `NANOCLAW_SLACK_BOT_TOKEN` + `NANOCLAW_SLACK_APP_TOKEN` |
| WhatsApp | Default built-in | Pass token via `NANOCLAW_WHATSAPP_TOKEN` env var                        |

NanoClaw's channels are code-driven — the runtime reads env vars and programmatically registers channel skills. ClawDen only needs to inject the right env vars.

#### PicoClaw (JSON config)

PicoClaw uses a JSON config file:

| Channel     | Config path                                         | Notes                    |
| ----------- | --------------------------------------------------- | ------------------------ |
| Telegram    | `config.telegram.token`                             | Native Go implementation |
| Discord     | `config.discord.token`                              | Native Go implementation |
| Slack       | `config.slack.bot_token` + `config.slack.app_token` | Native Go implementation |
| Feishu/Lark | `config.feishu.app_id` + `config.feishu.app_secret` | Native Go implementation |

ClawDen generates the JSON config file and mounts it. PicoClaw reads `config.json` from its working directory.

### Channel Proxy: Every Agent on Every Channel

If a runtime doesn't natively support a channel (see matrix above), ClawDen proxies automatically:

```
User (Telegram) ──► ClawDen Channel Proxy
                         │
                   ┌─────┴─────┐
                   │           │
            [native]       [proxied]
                   │           │
              ZeroClaw    NullClaw
           (has Telegram) (ClawDen bridges
                          via CRI send())
```

Users see a "proxied" indicator in `clawden ps` but otherwise it just works. No config difference.

### Channel Instance Validation

ClawDen enforces these rules at startup:

1. **One instance, one runtime.** A channel instance name can only appear in one runtime's `channels` list. If `support-tg` is assigned to both `zeroclaw` and `picoclaw` → startup error.
2. **One token, one instance.** Two channel instances of the same type cannot resolve to the same token value. This catches copy-paste mistakes (e.g., both `support-tg` and `creative-tg` pointing to the same `$BOT_TOKEN`). Same resolved token across instances of the same type → startup error.
3. **Type must be valid.** If `type` can't be inferred from name and isn't explicitly set → startup error.
4. **Referenced channels must exist.** Runtime references a channel name not in `channels:` → startup error.

Example error messages:
```
Error: Channel 'support-tg' is assigned to both 'zeroclaw' and 'picoclaw'.
       Each channel instance can only connect to one runtime.

Error: Channels 'support-tg' and 'creative-tg' resolve to the same telegram token.
       Each bot token can only be used by one channel instance.

Error: Channel 'my-chat' has no 'type' field and 'my-chat' is not a known channel type.
       Add 'type: telegram' (or another supported type) to the channel config.

Error: Runtime 'zeroclaw' references channel 'slack-bot' which is not defined in 'channels:'.
```

### Channel CLI Commands

```bash
clawden channels           # list configured channels + connection status
clawden channels test      # test all channel credentials
clawden channels test telegram  # test just telegram
```

### Auth & Security

- **Allowlist model**: Empty = deny all (safe default), `["*"]` = allow all, else exact-match
- Configured per-channel instance in YAML:

```yaml
channels:
  support-tg:
    type: telegram
    token: $SUPPORT_TG_TOKEN
    allowed_users: ["12345", "67890"]   # optional
  team-discord:
    type: discord
    token: $DISCORD_BOT_TOKEN
    allowed_roles: ["admin"]            # optional
```

- Credentials encrypted at rest, never in logs or API responses

## Plan

### Phase 1a: Tier 1 Channels (Telegram, Discord, Feishu/Lark)
- [ ] Define channel instance schema (name, type inference, per-type fields)
- [ ] Implement channel instance validation (1:1 instance-runtime, token uniqueness, type resolution, reference checks)
- [ ] Implement channel credential resolver ($ENV_VAR + .env auto-load)
- [ ] Add OpenClaw credential mapping for Tier 1 channels (grammY, discord.js, Feishu SDK) — highest priority
- [ ] Add ZeroClaw credential mapping for Tier 1 channels (env vars, TOML)
- [ ] Add NanoClaw credential mapping for Tier 1 channels (skill injection, env vars)
- [ ] Add PicoClaw credential mapping for Tier 1 channels (JSON config)
- [ ] Implement `clawden channels` and `clawden channels test` CLI commands
- [ ] Channel health monitoring

### Phase 1b: Tier 2 Channels (Slack, WhatsApp)
- [ ] Add Slack credential mapping across Phase 1 runtimes (dual token: bot + app)
- [ ] Add WhatsApp credential mapping across Phase 1 runtimes (Baileys, Meta API, native Go, default)
- [ ] Implement channel proxy for unsupported runtime+channel combos

### Phase 1c: Tier 3 Channels (Signal, DingTalk, QQ, etc.)
- [ ] Add Signal credential mapping (OpenClaw + ZeroClaw native; proxy for NanoClaw, PicoClaw)
- [ ] Add DingTalk credential mapping (PicoClaw native; proxy for others)
- [ ] Add QQ credential mapping (PicoClaw native; proxy for others)
- [ ] Validate channel proxy across all Tier 3 channels

### Phase 2: IronClaw, NullClaw, MicroClaw & Dashboard
- [ ] Add IronClaw credential mapping (WASM capabilities, secret injection)
- [ ] Add NullClaw credential mapping (JSON config)
- [ ] Add MicroClaw credential mapping (YAML config)
- [ ] Add channel status to dashboard (spec 021)

## Test

- [ ] `clawden.yaml` with telegram channel instance + ZeroClaw runtime connects and responds
- [ ] $ENV_VAR references resolve correctly from environment and .env file
- [ ] Same channel instance assigned to two runtimes → clear error at startup
- [ ] Same resolved token across two instances of the same type → clear error at startup
- [ ] Channel instance with unknown name and no `type` field → clear error at startup
- [ ] Runtime references undefined channel instance → clear error at startup
- [ ] Type inference works: instance name `telegram` → type `telegram` without explicit `type` field
- [ ] Multiple telegram instances with different tokens + different runtimes → works correctly
- [ ] Channel proxy bridges Telegram to a runtime without native support
- [ ] `clawden channels test` validates credentials without starting runtimes
- [ ] Credentials never appear in logs or `clawden ps` output
- [ ] Allowlist correctly restricts who can message the agent

## Notes

- **OpenClaw is Phase 1 top priority**: Most popular runtime — complex channel story (10+ channels, grammY, discord.js, Baileys, Bolt, JSON5 config) but critical to support first
- **IronClaw deferred to Phase 2**: WASM-based channels are the most secure (sandboxed) but limited set and lower adoption. Proxy covers the gap in the meantime
- **NullClaw, MicroClaw deferred to Phase 2**: Lower priority runtimes, straightforward credential mapping when needed
- **Telegram is universal** — all runtimes support it. Best default for testing
- **Config format divergence is internal**: JSON5 (OpenClaw), TOML (ZeroClaw), code-driven (NanoClaw), JSON (PicoClaw). Users never see this — they write one `clawden.yaml`
- **Future: channel routing** — single bot token → ClawDen webhook ingress → route by conversation context. Deferred, too complex for now
- **Channel instances vs channel types** — the 1:1 relationship is between instances and runtimes, not types and runtimes. Multiple instances of the same type (e.g., two Telegram bots) are a common multi-agent pattern