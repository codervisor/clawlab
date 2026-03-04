---
status: complete
created: 2026-03-04
priority: high
tags:
- cli
- ux
- bug-fix
- credential-validation
- developer-experience
- run
created_at: 2026-03-04T06:38:25.829298791Z
updated_at: 2026-03-04T07:20:20.792058269Z
completed_at: 2026-03-04T07:20:20.792058269Z
transitions:
- status: complete
  at: 2026-03-04T07:20:20.792058269Z
---

# Run Command UX Polish — Credential Resolution, Smart Defaults & Actionable Errors

## Overview

`clawden run` has credential shortcut flags (`--token`, `--api-key`, `--provider`) that work correctly when a `clawden.yaml` exists but **silently break without one**. The validation layer checks the config struct instead of the resolved env vars, causing `--token` to be ignored during validation even though the env var is correctly set. Beyond this bug, the error messages lack actionable context: they don't show what's already available in the host environment, don't suggest smart defaults, and don't provide copy-paste-ready fix commands.

This spec fixes the validation bug and elevates the `clawden run` error UX from "what's missing" to "here's exactly how to fix it."

## Context

### The Bug

When running without `clawden.yaml` (the zero-config quickstart path):

```sh
clawden run --token 123:abc --channel telegram zeroclaw
```

**Expected**: Runtime starts with `TELEGRAM_BOT_TOKEN=123:abc`
**Actual**: Error says `TELEGRAM_BOT_TOKEN ...... missing`

**Root cause**: `validate_direct_runtime_config()` (up.rs) checks `channel.token` on the config struct, not the `env_vars` vec. Without a YAML file, `apply_run_overrides()` is skipped (config is `None`), and the channel entry is later created via `empty_channel_instance()` with `token: None`. The env var IS set correctly by `apply_shortcut_env_overrides()`, but validation never looks at it.

### User Story (verbatim reproduction)

```sh
# Attempt 1: Basic run → missing token (expected, no token given)
$ clawden run --channel telegram zeroclaw
Error: TELEGRAM_BOT_TOKEN ...... missing

# Attempt 2: Token provided → STILL missing (BUG)
$ clawden run --token $BOT_TOKEN --channel telegram zeroclaw
Error: TELEGRAM_BOT_TOKEN ...... missing

# Attempt 3: Full flags → STILL missing (BUG)
$ clawden run --api-key $OPENROUTER_API_KEY --provider openrouter \
    --token $BOT_TOKEN --channel telegram zeroclaw
Error: TELEGRAM_BOT_TOKEN ...... missing
```

The user gave all required info, but the CLI rejected every attempt.

### Additional UX Gaps

1. **No smart host-env detection**: `OPENROUTER_API_KEY` is already set in the shell, but the error doesn't mention it or offer to use it
2. **No default provider**: Most users use OpenRouter (per runtime defaults); `clawden run` should infer a default when the provider isn't configured
3. **Error doesn't show what IS resolved**: Only shows "missing" fields, never "provided" or "detected" fields — user can't tell what worked
4. **No example fix commands**: Error says "pass `--api-key`" but doesn't show the actual command to copy-paste
5. **Provider hint is stderr-only**: When `--api-key` is used without `--provider`, the hint goes to stderr and can be missed
6. **No host-env-to-provider inference**: If `OPENROUTER_API_KEY` is in the environment, the CLI could infer `--provider openrouter` automatically
7. **Channel validation doesn't use env_vars**: The env_vars parameter in `validate_direct_runtime_config` is only checked for `CLAWDEN_LLM_API_KEY`, not for channel tokens — inconsistent design

### Dependencies

- 034-cli-runtime-ergonomics (complete) — introduced the shortcut flags and validation. This spec fixes bugs and improves UX on top of that foundation
- 025-llm-provider-api-key-management (complete) — provider env var mappings used for host-env detection

## Design

### 1. Fix Validation to Check Env Vars for Channel Tokens (Bug Fix)

`validate_direct_runtime_config()` must check the `env_vars` vec for channel tokens in addition to (or instead of) the config struct fields. This makes validation source-agnostic: it doesn't matter whether the token came from `clawden.yaml`, `--token`, or `-e`.

**Implementation**: For each channel, after checking `channel.token`/`channel.bot_token` on the config struct, also check the env_vars vec for the canonical env var name (`TELEGRAM_BOT_TOKEN`, `DISCORD_BOT_TOKEN`, etc.). If found and non-empty, the credential is satisfied.

**Channels and their env var checks**:

| Channel | Config struct fields | Env var fallbacks |
|---|---|---|
| telegram | `token`, `bot_token` | `TELEGRAM_BOT_TOKEN` |
| discord | `token`, `bot_token` | `DISCORD_BOT_TOKEN` |
| slack | `bot_token` + `app_token` | `SLACK_BOT_TOKEN` + `SLACK_APP_TOKEN` |
| signal | `phone` + `token` | `SIGNAL_PHONE` + `SIGNAL_TOKEN` |
| other | `token`, `bot_token` | `{CHANNEL}_BOT_TOKEN` |

### 2. Apply CLI Overrides Even Without clawden.yaml

When `config` is `None` and CLI credential flags are present (`--token`, `--api-key`, `--provider`, `--model`, `--system-prompt`), create a minimal config and apply `apply_run_overrides()` before building env vars. This ensures the config struct AND env vars are consistent.

**Implementation**: Move the `apply_run_overrides` call after the `config.get_or_insert_with()` block that creates the empty config for CLI-only runs, or call it a second time after config creation.

### 3. Host Environment Auto-Detection

Before validation, scan the host process environment for well-known credential env vars and report them in the error summary:

**Provider keys to scan**:
- `OPENROUTER_API_KEY` → provider: openrouter
- `OPENAI_API_KEY` → provider: openai
- `ANTHROPIC_API_KEY` → provider: anthropic
- `GEMINI_API_KEY`, `GOOGLE_API_KEY` → provider: google
- `MISTRAL_API_KEY` → provider: mistral
- `GROQ_API_KEY` → provider: groq

**Channel tokens to scan**:
- `TELEGRAM_BOT_TOKEN`
- `DISCORD_BOT_TOKEN`
- `SLACK_BOT_TOKEN`, `SLACK_APP_TOKEN`

If a credential is found in the host environment but not in the resolved config/env, show it as "detected in environment" and auto-inject it into the runtime env vars.

**Behavior**: Host environment values are injected at lower precedence than all other sources (below `.env` and `clawden.yaml`). They serve as a convenience fallback for the zero-config quickstart, not as a replacement for explicit configuration.

### 4. Default Provider Inference

When no provider is configured (no `clawden.yaml`, no `--provider`), infer the provider from available host environment variables:

**Priority order** (first match wins):
1. `OPENROUTER_API_KEY` set → default to `openrouter`
2. `OPENAI_API_KEY` set → default to `openai`
3. `ANTHROPIC_API_KEY` set → default to `anthropic`
4. `GEMINI_API_KEY` / `GOOGLE_API_KEY` set → default to `google`
5. `MISTRAL_API_KEY` set → default to `mistral`
6. `GROQ_API_KEY` set → default to `groq`

When a provider is inferred, print an info line:
```
ℹ Using provider openrouter (detected OPENROUTER_API_KEY in environment)
```

If multiple provider keys are detected, use the priority order above and print which was selected.

### 5. Improved Error Messages with Actionable Context

Replace the current error format with a richer summary that shows:
- What IS resolved (and from where)
- What is MISSING
- Concrete fix commands

**Current format** (minimal):
```
Required fields for this run:
    channel: telegram
        - TELEGRAM_BOT_TOKEN ...... missing

How to continue:
    1) Provide missing fields now: --api-key ..., --token ..., -e KEY=VAL, or --env-file <path>
    2) Skip credential validation for this run: --allow-missing-credentials
```

**New format** (actionable):
```
Required fields for this run:
    provider: (none configured)
        - LLM API key ............ ✗ missing
        💡 Detected OPENROUTER_API_KEY in your environment — add --provider openrouter to use it
    channel: telegram
        - TELEGRAM_BOT_TOKEN ..... ✓ provided (--token)

Suggested command:
    clawden run --provider openrouter --token <your-token> --channel telegram zeroclaw

Or skip validation:
    clawden run --allow-missing-credentials --channel telegram zeroclaw
```

**Design principles for the new format**:
- Show ✓ for resolved fields and ✗ for missing fields
- Show the source of resolved values: `(--token)`, `(clawden.yaml)`, `(.env)`, `(environment)`
- When host env vars are detected that could fill gaps, show a 💡 hint
- Always end with a concrete `Suggested command:` line that the user can copy-paste
- Keep the `--allow-missing-credentials` escape hatch

### 6. Provider & API Key Guidance in Error Output

When the error involves a missing provider or API key:
- List known providers: `openai, anthropic, openrouter, google, mistral, groq, ollama`
- Show which provider env vars are detected in the host environment
- If no provider is configured and no env vars detected, suggest the most common setup:
  ```
  💡 No provider configured. Try: --provider openrouter --api-key <key>
     Or set OPENROUTER_API_KEY in your environment / .env file
  ```

### 7. System Environment Summary (`clawden config env`)

Add a subcommand to show detected environment variables relevant to ClawDen:

```sh
$ clawden config env
Detected environment variables:
    OPENROUTER_API_KEY .... ✓ set (sk-or-v1-***...redacted)
    OPENAI_API_KEY ........ ✗ not set
    ANTHROPIC_API_KEY ..... ✗ not set
    TELEGRAM_BOT_TOKEN .... ✗ not set
    DISCORD_BOT_TOKEN ..... ✗ not set
```

**Behavior**:
- Scans for all well-known ClawDen/provider/channel env vars in the current shell
- Redacts values by default (shows first 8 chars + `***`)
- `--reveal` flag to show full values
- Grouped by category: LLM Providers, Channel Tokens, ClawDen Config

## Plan

- [x] Fix `validate_direct_runtime_config()` to check env_vars vec for channel tokens (TELEGRAM_BOT_TOKEN, DISCORD_BOT_TOKEN, SLACK_BOT_TOKEN, SLACK_APP_TOKEN, SIGNAL_PHONE, SIGNAL_TOKEN)
- [x] Apply CLI overrides (`apply_run_overrides`) even when `clawden.yaml` doesn't exist — create minimal config first
- [x] Add host-env scanning for well-known provider API key env vars
- [x] Add host-env scanning for well-known channel token env vars
- [x] Auto-inject detected host-env credentials into runtime env at lowest precedence
- [x] Implement default provider inference from host env vars (priority: openrouter > openai > anthropic > google > mistral > groq)
- [x] Print info line when provider is auto-inferred from environment
- [x] Redesign validation error format: show ✓/✗ status, source labels, detected env hints
- [x] Add concrete `Suggested command:` line to error output
- [x] Add provider/API key guidance section to error output (list known providers, show detected env vars)
- [x] Add `clawden config env` subcommand (scan and display known env vars with redaction)
- [x] Add tests: `--token` works without `clawden.yaml`
- [x] Add tests: host env `OPENROUTER_API_KEY` auto-detected and injected
- [x] Add tests: default provider inferred from host env
- [x] Add tests: error message includes ✓/✗ status and source labels
- [x] Add tests: `clawden config env` output format
- [x] Add tests: precedence — explicit `--provider` overrides auto-inferred provider

## Test

- [x] `clawden run --token tok --channel telegram zeroclaw` (no `clawden.yaml`) → starts successfully with `TELEGRAM_BOT_TOKEN=tok`
- [x] `clawden run --api-key sk-... --provider openrouter --token tok --channel telegram zeroclaw` (no `clawden.yaml`) → starts successfully
- [x] `OPENROUTER_API_KEY=sk-... clawden run --channel telegram --token tok zeroclaw` (no `clawden.yaml`, no `--provider`) → infers openrouter, starts successfully
- [x] `OPENROUTER_API_KEY=sk-... OPENAI_API_KEY=sk-... clawden run zeroclaw` → infers openrouter (higher priority), prints info line
- [x] `OPENROUTER_API_KEY=sk-... clawden run --provider openai zeroclaw` → explicit `--provider` wins over auto-inference
- [x] `clawden run --channel telegram zeroclaw` (no token anywhere) → error shows `TELEGRAM_BOT_TOKEN ..... ✗ missing` with suggested command
- [x] `clawden run --token tok --channel telegram zeroclaw` (no provider) → error shows detected env vars if any, suggests `--provider`
- [x] `clawden config env` → lists all known env vars with ✓/✗ status
- [x] `clawden config env --reveal` → shows full env var values
- [x] Error message includes copy-paste-ready `Suggested command:` line
- [x] Error message shows source of resolved values (`--token`, `clawden.yaml`, `.env`, environment)
- [x] `--allow-missing-credentials` still works and skips all validation

## Notes

### Precedence (updated from spec 034)

With host-env auto-detection, the full precedence order becomes:

1. Explicit CLI key-value overrides (`-e KEY=VAL`) — highest
2. Shortcut CLI flags (`--api-key`, `--token`, `--provider`, etc.)
3. Explicit env file (`--env-file`)
4. Auto-detected `.env` file
5. `clawden.yaml`
6. Host process environment auto-detection — lowest (new)

### Non-Goals

- **Credential validation against remote APIs** — checking that a Telegram token is actually valid by calling the Telegram API. This spec only validates that required fields have non-empty values.
- **Interactive prompts** — `clawden run` remains non-interactive. Use `clawden init` for interactive setup.
- **Changing the default provider globally** — this spec only infers a default from available env vars. There is no hardcoded "openrouter is always the default" behavior.
- **Runtime-specific env var auto-detection** — only canonical ClawDen/provider/channel env vars are scanned, not runtime-specific variants like `ZEROCLAW_TELEGRAM_BOT_TOKEN`.
