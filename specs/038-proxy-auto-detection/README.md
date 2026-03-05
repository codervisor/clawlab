---
status: in-progress
created: 2026-03-05
priority: high
tags:
- proxy
- config
- bug
- cli
- corporate-network
- developer-experience
created_at: 2026-03-05T01:43:22.476304293Z
updated_at: 2026-03-05T01:43:29.124393775Z
transitions:
- status: in-progress
  at: 2026-03-05T01:43:29.124393775Z
---

# HTTP Proxy Auto-Detection & Config Injection for Runtime Processes

## Overview

When `clawden run` launches a runtime behind a corporate HTTP proxy, the runtime fails to reach external APIs (Telegram, LLM providers, etc.) even when the user passes proxy env vars via `-e http_proxy -e https_proxy -e no_proxy`. The runtime's own config file disables proxy support by default, overriding the inherited environment variables.

## Context

### Root Cause Chain

The failure involves three compounding issues:

1. **Runtime template defaults disable proxy**: When `zeroclaw onboard` generates a template `config.toml`, it sets `[proxy] enabled = false`. ClawDen's `generate_toml_config()` seeds from this template via `seed_template_config()`, inheriting the disabled proxy.

2. **No host proxy auto-detection**: `generate_toml_config()` translates provider keys, channel tokens, model settings — but has no awareness of HTTP proxy environment variables (`HTTP_PROXY`, `HTTPS_PROXY`, `NO_PROXY`). The `[proxy]` section is never populated from the host environment.

3. **Wrong proxy scope**: Even if `[proxy] enabled = true` were set manually, the template default `scope = "zeroclaw"` only applies the proxy to zeroclaw's main LLM client. Channel HTTP clients (Telegram polling, Discord gateway, etc.) bypass the proxy entirely because they are not in the "zeroclaw" scope. The correct scope is `"environment"`, which applies proxy settings process-wide to all HTTP clients.

### Why `-e http_proxy` Alone Doesn't Work

Rust's `std::process::Command` inherits the parent environment by default (no `env_clear()` in production code), and `parse_env_overrides()` correctly reads key-only entries from the host env. So the proxy env vars **are** present in the child process.

However, runtimes like zeroclaw call `reqwest::Client::builder().no_proxy()` when `proxy.enabled = false` in their config, explicitly stripping proxy support from the HTTP client. The config file overrides environment variables.

This is the same class of bug as spec 031 (Direct Mode Config Injection), where the runtime's config file silently overrides what ClawDen intended.

### Affected Runtimes

| Runtime   | Config Format | Has `[proxy]` section | Affected |
| --------- | ------------- | --------------------- | -------- |
| zeroclaw  | TOML          | Yes                   | ✅        |
| nullclaw  | TOML          | Yes (likely)          | ✅        |
| openfang  | TOML          | Yes (likely)          | ✅        |
| picoclaw  | JSON          | Unknown               | ❓        |
| openclaw  | env-only      | N/A (uses env vars)   | ❌        |
| nanoclaw  | env-only      | N/A (uses env vars)   | ❌        |

### Environment Variables Involved

**Standard (read by reqwest/curl):**
- `HTTP_PROXY` / `http_proxy`
- `HTTPS_PROXY` / `https_proxy`
- `NO_PROXY` / `no_proxy`
- `ALL_PROXY` / `all_proxy`

**ZeroClaw-specific:**
- `ZEROCLAW_PROXY_ENABLED`
- `ZEROCLAW_HTTP_PROXY` / `ZEROCLAW_HTTPS_PROXY` / `ZEROCLAW_ALL_PROXY`
- `ZEROCLAW_NO_PROXY`
- `ZEROCLAW_PROXY_SCOPE` (valid: `environment` | `zeroclaw` | `services`)

## Solution

### Config Generation (`config_gen.rs`)

Add `inject_proxy_config()` to `generate_toml_config()`:
- Detect `http_proxy`/`HTTPS_PROXY`/`NO_PROXY` (both cases) from the host environment
- When any proxy URL is detected, set `[proxy] enabled = true`
- Set `scope = "environment"` (not "zeroclaw") so all HTTP clients in the process use the proxy
- Populate `http_proxy`, `https_proxy`, and `no_proxy` fields from detected values
- Runs after `seed_template_config()` so it overrides the template's `enabled = false`

### Behavior

- **Automatic**: No user action required — proxy is detected and injected transparently
- **No-op in clean environments**: When no proxy env vars are set, `inject_proxy_config()` returns without modifying the config
- **User overrides preserved**: `merge_json_into_toml()` (config overrides from `clawden.yaml`) runs after injection, so explicit `config.proxy.*` settings in `clawden.yaml` take precedence

## Checklist

- [x] Add `inject_proxy_config()` to `generate_toml_config()` in `config_gen.rs`
- [x] Detect `http_proxy`/`HTTPS_PROXY` from host env (both cases)
- [x] Set `proxy.enabled = true` and `proxy.scope = "environment"` when proxy detected
- [x] Populate `proxy.http_proxy`, `proxy.https_proxy`, `proxy.no_proxy` from env
- [x] Verify generated config.toml contains correct `[proxy]` section
- [x] Verify Telegram channel connects through proxy (no startup probe error)
- [x] All clawden-cli tests pass
- [ ] Consider picoclaw proxy config injection (if applicable)
- [ ] Consider Docker mode proxy passthrough (env vars to container)

## Notes

<!-- Optional: Research findings, alternatives considered, open questions -->
