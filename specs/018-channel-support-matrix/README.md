---
status: planned
created: 2026-02-26
priority: medium
tags:
- channels
- messaging
- integration
- telegram
- discord
- whatsapp
parent: 009-orchestration-platform
depends_on:
- 010-claw-runtime-interface
created_at: 2026-02-26T02:50:41.154596674Z
updated_at: 2026-02-26T02:50:41.154596674Z
---

# Chat Channel Support Matrix & Unified Channel Layer

## Overview

Every claw runtime has independently built its own messaging integrations — Telegram bots, Discord adapters, WhatsApp bridges, etc. The result is fragmented: OpenClaw supports 14+ channels, NullClaw supports 17, PicoClaw supports 6, but each uses different libraries, auth flows, and configuration patterns. ClawDen needs a unified channel layer so operators can configure messaging channels once and route them to any runtime in the fleet.

This spec documents which channels each runtime supports (the compatibility matrix) and designs ClawDen's channel abstraction for cross-runtime message routing.

## Channel Support Matrix

Data sourced from official GitHub repos (February 2026).

### By Runtime

| Channel | OpenClaw | ZeroClaw | PicoClaw | NanoClaw | IronClaw | NullClaw | MicroClaw | MimiClaw |
|---------|:--------:|:--------:|:--------:|:--------:|:--------:|:--------:|:---------:|:--------:|
| **CLI / REPL** | ✅ | ✅ | ✅ | — | ✅ | ✅ | — | ✅ (serial) |
| **Telegram** | ✅ | ✅ | ✅ | ✅ | ✅ (WASM) | ✅ | ✅ | ✅ |
| **Discord** | ✅ | ✅ | ✅ | ✅ | — | ✅ | ✅ | — |
| **WhatsApp** | ✅ (Baileys) | ✅ (Meta API) | — | ✅ (Baileys) | — | ✅ | ✅ | — |
| **Slack** | ✅ (Bolt) | — | — | ✅ | ✅ (WASM) | ✅ | ✅ | — |
| **Signal** | ✅ (signal-cli) | — | — | ✅ | — | ✅ | ✅ | — |
| **iMessage** | ✅ (BlueBubbles + legacy) | — | — | — | — | ✅ | ✅ | — |
| **Matrix** | — | — | — | — | — | ✅ | ✅ | — |
| **Google Chat** | ✅ | — | — | — | — | — | — | — |
| **Microsoft Teams** | ✅ | — | — | — | — | — | — | — |
| **DingTalk** | — | — | ✅ | — | — | ✅ | ✅ | — |
| **LINE** | — | — | ✅ | — | — | ✅ | — | — |
| **Lark / Feishu** | — | — | — | — | — | ✅ | ✅ | — |
| **QQ** | — | — | ✅ | — | — | ✅ | ✅ | — |
| **WeCom** | — | — | ✅ | — | — | — | — | — |
| **IRC** | — | — | — | — | — | ✅ | ✅ | — |
| **Nostr** | — | ✅ | — | — | — | — | ✅ | — |
| **Email** | — | — | — | — | — | ✅ | ✅ | — |
| **Nextcloud Talk** | — | ✅ | — | — | — | — | — | — |
| **Zalo** | ✅ | — | — | — | — | — | — | — |
| **OneBot** | — | — | — | — | — | ✅ | — | — |
| **Linq** | — | ✅ | — | — | — | — | — | — |
| **WebChat / Web UI** | ✅ | — | — | — | ✅ (SSE/WS) | — | ✅ | ✅ (WS) |
| **Webhook (generic)** | ✅ | — | — | — | ✅ | ✅ | — | — |
| **MaixCam** | — | — | — | — | — | ✅ | — | — |
| **Mattermost** | — | — | — | — | — | ✅ | — | — |
| **Total channels** | **14+** | **6** | **6** | **5** | **4** | **17** | **13** | **2** |

### By Channel (Coverage Across Runtimes)

| Channel | Runtimes Supporting It | Notes |
|---------|----------------------|-------|
| Telegram | 8/8 (all) | Universal — every runtime supports it. Best candidate for "default" channel |
| Discord | 6/8 | Missing: IronClaw, MimiClaw |
| WhatsApp | 4/8 | Two implementations: Baileys (OpenClaw, NanoClaw) vs Meta Cloud API (ZeroClaw, NullClaw) |
| CLI/REPL | 6/8 | Local-only, not routable through ClawDen |
| Slack | 5/8 | Bolt SDK (OpenClaw), Socket Mode or webhook-based (others) |
| Signal | 4/8 | Requires `signal-cli` daemon |
| WebChat | 4/8 | Each runtime has its own Web UI approach |

### Implementation Libraries by Runtime

| Runtime | Telegram | Discord | WhatsApp | Slack |
|---------|----------|---------|----------|-------|
| OpenClaw | grammY | discord.js | Baileys | Bolt |
| ZeroClaw | native Rust | native Rust | Meta Cloud API | — |
| PicoClaw | native Go | native Go | — | — |
| NanoClaw | (via skills) | (via skills) | Baileys | (via skills) |
| IronClaw | WASM channel | — | — | WASM channel |
| NullClaw | native Zig | native Zig | native Zig | native Zig |
| MicroClaw | native Rust | native Rust | native Rust | native Rust |
| MimiClaw | native C (ESP HTTP) | — | — | — |

## Design

### ClawDen Channel Architecture

ClawDen doesn't replace each runtime's native channel implementation. Instead, it provides:

1. **Channel Registry** — Tracks which channels each agent supports and their current state
2. **Channel Proxy** (optional) — For runtimes that lack a specific channel, ClawDen can act as a proxy: receive on the channel, translate to the runtime's API, relay the response back
3. **Unified Config** — Single place to configure channel credentials (bot tokens, API keys), mapped to each runtime's native config format via the config translator (spec 013)

```
User (Telegram) ──► ClawDen Channel Router
                         │
                   ┌─────┴─────┐
                   │           │
            [native channel] [proxy mode]
                   │           │
              ZeroClaw    IronClaw
           (has Telegram) (no Telegram,
                          ClawDen proxies)
```

### Channel Proxy Mode

For runtimes that don't natively support a channel, ClawDen can bridge:

1. ClawDen receives the message on the channel (e.g., Telegram)
2. Translates to a `send()` call on the CRI adapter
3. Gets the `AgentResponse` back
4. Sends the response back on the channel

This means every agent in the fleet is reachable on every channel, even if the runtime doesn't natively support it.

### Channel Credential Management

All channel credentials flow through ClawDen's secret vault (spec 013):
- Bot tokens, API keys, webhook secrets stored encrypted
- Injected into runtime configs at deploy time via env vars
- Never exposed in logs or API responses
- Rotatable without redeploying containers

### Auth & Security Patterns

Every runtime uses some form of allowlisting:
- **Allowlist model**: Empty = deny all, `["*"]` = allow all, else exact-match (universal across all runtimes)
- **Pairing**: OpenClaw and some others use a pairing code flow for DM access
- **Group activation**: Mention-only vs always-respond (configurable per-channel)

ClawDen normalizes these into a canonical security policy per agent.

## Plan

- [ ] Audit and document the complete channel matrix (this spec captures the initial audit)
- [ ] Design canonical channel config schema in `clawden-config` (credentials + allowlists + policies)
- [ ] Implement channel config translators per runtime in the CRI adapters
- [ ] Build channel proxy in `clawden-server` for bridging unsupported channels
- [ ] Implement channel health monitoring (is the Telegram bot connected? Is Discord auth valid?)
- [ ] Add channel management to the dashboard (spec 014) — enable/disable, status, logs per channel
- [ ] Document channel setup guides for operators

## Test

- [ ] Channel matrix accurately reflects each runtime's current capabilities
- [ ] Canonical channel config round-trips through each runtime's translator
- [ ] Channel proxy can bridge a Telegram message to a runtime that lacks native Telegram support
- [ ] Channel credentials are encrypted at rest and never appear in logs
- [ ] Channel health monitor detects a disconnected bot token
- [ ] Dashboard shows real-time channel status per agent

## Notes

- **Telegram is the universal channel** — all 8 runtimes support it. It's the safest default for testing and the best candidate for ClawDen's proxy implementation
- **WhatsApp fragmentation** — two incompatible approaches: Baileys (unofficial, full-featured, used by OpenClaw/NanoClaw) vs Meta Cloud API (official, webhook-based, used by ZeroClaw). ClawDen should support both
- **NanoClaw's skill-based channels** — NanoClaw adds channels via Claude Code skills (`/add-telegram`, `/add-slack`), not built-in code. This means channel support varies per fork. ClawDen should track announced skills, not just core code
- **IronClaw's WASM channels** — channels are compiled to WebAssembly for sandboxed execution. Unique approach that's more secure but harder to extend
- **NullClaw has the broadest channel support** (17 channels) despite being the newest and smallest binary (678 KB). All channels are native Zig vtable implementations
- **Chinese IM ecosystem** — DingTalk, Lark/Feishu, QQ, WeCom, and Zalo are important for APAC deployment. PicoClaw (from Sipeed, a Chinese hardware company) and NullClaw have the best coverage here
- **MimiClaw is Telegram-only** by hardware constraint (ESP32-S3 WiFi + HTTP). Adding more channels would require more flash/RAM than a $5 chip can provide
