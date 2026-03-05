---
status: complete
created: 2026-03-05
priority: high
tags:
- openclaw
- config
- bug
- provider
- auth
- cli
depends_on:
- 031-direct-mode-config-injection
created_at: 2026-03-05T07:56:16.979863538Z
updated_at: 2026-03-05T07:56:27.485701421Z
completed_at: 2026-03-05T07:56:27.485701421Z
---

# OpenClaw Provider-Model Routing — Correct Auth Resolution for Non-Anthropic Providers

## Overview

When `clawden run openclaw` uses a routing provider like OpenRouter, OpenClaw fails to authenticate because it extracts the provider from the model string prefix (e.g. `anthropic` from `anthropic/claude-opus-4-6`) and tries to find credentials for that provider — not the actual configured provider. ClawDen injects `OPENROUTER_API_KEY` into the environment, but OpenClaw looks for `ANTHROPIC_API_KEY` and throws `No API key found for provider "anthropic"`.

## Motivation

- `clawden run --channel telegram openclaw` with `OPENROUTER_API_KEY` set auto-detects provider correctly and logs `Using provider openrouter`, but openclaw crashes on first message
- The error is confusing: it mentions `auth-profiles.json` and `openclaw agents add` — concepts users of `clawden run` shouldn't need to know about
- ZeroClaw handles this transparently because it has separate `default_provider` and `default_model` config fields; OpenClaw encodes the provider in the model ref string itself
- This is the most common failure mode for new users who have an OpenRouter key and try `clawden run openclaw`

## Root Cause

OpenClaw's auth resolution chain for LLM requests:

1. Parse model ref `anthropic/claude-opus-4-6` → extracts `anthropic` as provider
2. Check `auth-profiles.json` for stored credentials → empty (ClawDen doesn't write this file)
3. Check env var `ANTHROPIC_API_KEY` → not set
4. Check `models.json` providers → no `anthropic` entry with real API key
5. **Throw: "No API key found for provider anthropic"**

ClawDen correctly sets `OPENROUTER_API_KEY` and `OPENCLAW_LLM_API_KEY` in the child process environment, but OpenClaw ignores these because it resolves auth per-provider based on the model ref prefix.

## Design

### Model Ref Re-Prefixing via `openclaw.json`

Inject `agents.defaults.model` into the generated `openclaw.json` config file with the model string re-prefixed through the configured provider:

- Config: `provider: openrouter`, `model: anthropic/claude-opus-4-6`
- Generated: `agents.defaults.model: "openrouter/anthropic/claude-opus-4-6"`

OpenClaw's `parseModelRef()` splits at the first `/`, extracting `openrouter` as the provider. Auth resolution then finds `OPENROUTER_API_KEY` from the environment and succeeds.

### Provider Auto-Detection Integration

This builds on the existing `infer_provider_from_host_env()` auto-detection in `run.rs` — no changes needed there. The flow:

1. `infer_provider_from_host_env()` detects `OPENROUTER_API_KEY` → sets `provider: openrouter` in config
2. `generate_openclaw_config()` reads provider via `runtime_provider_and_model()`
3. New `inject_openclaw_agent_model()` re-prefixes the model string through the detected provider
4. OpenClaw parses the correct provider and authenticates via environment

### Rules

- **Skip when provider is `anthropic`**: OpenClaw's built-in default is `anthropic/claude-opus-4-6`, so no override is needed.
- **Respect user overrides**: If the user sets `agents.defaults.model` via `config:` in `clawden.yaml`, don't overwrite it.
- **Handle missing model**: When no model is explicitly configured but provider is non-anthropic, use OpenClaw's default model `anthropic/claude-opus-4-6` re-prefixed through the provider.
- **Don't double-prefix**: If the model already starts with the provider slug (e.g. `openrouter/anthropic/...`), skip prefixing.

### Comparison with Other Runtimes

| Runtime | Provider Field | Model Field | Auth Mechanism |
|---|---|---|---|
| ZeroClaw | `default_provider` in config.toml | `default_model` (separate) | `reliability.api_keys` array |
| PicoClaw | `llm.provider` in config.json | `llm.model` (separate) | `llm.apiKeyRef` |
| OpenClaw | Encoded in model ref prefix | `agents.defaults.model` (combined) | Per-provider env vars / auth-profiles.json |

OpenClaw is unique in encoding the provider inside the model string. This spec bridges that gap.

## Implementation

### Files Changed

- **`crates/clawden-cli/src/commands/config_gen.rs`**: Add `inject_openclaw_agent_model()` function, called at the end of `generate_openclaw_config()`. Uses `runtime_provider_and_model()` to get the configured provider and model, then writes `agents.defaults.model` with the re-prefixed model ref.

## Plan

- [x] Add `inject_openclaw_agent_model()` to `config_gen.rs`
- [x] Call it from `generate_openclaw_config()` after channel and config override processing
- [x] Handle edge cases: anthropic provider skip, user override respect, missing model fallback, double-prefix prevention
- [x] Add unit tests for OpenRouter prefixing, default model fallback, anthropic skip, and user override

## Test

- [x] `provider: openrouter`, `model: anthropic/claude-opus-4-6` → `agents.defaults.model` = `"openrouter/anthropic/claude-opus-4-6"`
- [x] `provider: openrouter`, no model → `agents.defaults.model` = `"openrouter/anthropic/claude-opus-4-6"` (fallback)
- [x] `provider: anthropic` → no `agents.defaults.model` injected
- [x] User sets `config.agents.defaults.model: "custom/model"` → user value preserved, no override
- [ ] E2E: `clawden run --channel telegram openclaw` with `OPENROUTER_API_KEY` set → openclaw authenticates successfully

## Notes

- OpenClaw's `models.json` contains `"apiKey": "OPENROUTER_API_KEY"` as a literal string label, not an actual key value. The real key comes from `process.env.OPENROUTER_API_KEY` via `resolveEnvApiKey()`.
- OpenClaw's `auth-profiles.json` is a per-agent credential store managed by `openclaw agents add`. ClawDen does not write to this file — env vars are the correct integration path.
- This pattern applies to any routing/proxy provider (OpenRouter, LiteLLM, Cloudflare AI Gateway) where the model string contains a sub-provider prefix.
