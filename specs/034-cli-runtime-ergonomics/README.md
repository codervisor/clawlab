---
status: complete
created: 2026-03-04
priority: high
tags:
- cli
- ux
- ergonomics
- run
- config
- developer-experience
depends_on:
- 033-product-positioning
- 031-direct-mode-config-injection
- 025-llm-provider-api-key-management
created_at: 2026-03-04T01:48:05.129647004Z
updated_at: 2026-03-04T03:08:13.240684388Z
completed_at: 2026-03-04T03:05:08.843770093Z
transitions:
- status: in-progress
  at: 2026-03-04T02:40:04.026681478Z
- status: complete
  at: 2026-03-04T03:05:08.843770093Z
---

# CLI Run-Time Ergonomics — Inline Credentials, Model Override & Config Show

## Overview

`clawden run` and `clawden up` work well when `clawden.yaml` + `.env` are fully configured, but the CLI lacks escape hatches for the most common ad-hoc workflows. Users cannot:

1. **Pass credentials inline** — no way to specify a Telegram bot token, API key, or any credential from the command line without editing `.env` or `clawden.yaml` first
2. **Override model/provider** — no way to quickly test a different model or provider without editing the YAML
3. **Pass arbitrary env vars** — no `-e KEY=VAL` flag like `docker run` offers
4. **See resolved config** — no way to inspect what config/env vars ClawDen will actually pass to a runtime
5. **Set a system prompt inline** — common need for quick testing
6. **Expose/map ports** — no way to forward runtime ports to the host

These gaps force users into a "edit YAML → run → repeat" loop for tasks that should be one-liners, undermining the `uv run`-style transparent execution model (spec 033).

## Context

Current CLI already supports channel/tool selection, docker bypass, detach, cleanup, and restart controls. This spec focuses on the missing ad-hoc ergonomics: inline credentials/env vars, provider/model overrides, system prompt override, env-file override, port mapping, and resolved-config inspection.

### User Stories

- Quick Telegram bot test without editing files first
- Zero-config quickstart using inline API key + token
- Temporary model/provider experiments without modifying `clawden.yaml`
- Inspecting fully resolved runtime config/env before launch
- Quick system-prompt injection for ad-hoc behavior testing

### Why This Matters

Spec 033 positions `clawden run` as the "`uv run` for claw agents." But `uv run` lets you override everything inline (`uv run --python 3.12 --with requests script.py`). ClawDen's `run` only controls channels and tools — the most important parameters (credentials, model, provider) require file edits.

## Design

### 1. Inline Environment Variables (`-e`)

Add `-e KEY=VAL` flag to `run` and `up`, matching Docker's convention:

```sh
clawden run -e TELEGRAM_BOT_TOKEN=123:abc -e OPENAI_API_KEY=sk-... zeroclaw
clawden up -e OPENAI_API_KEY=sk-...
```

**Behavior**:
- Parsed as `KEY=VALUE` pairs, injected into the runtime's env alongside `build_runtime_env_vars()` output
- `-e` values take precedence over `.env` and `clawden.yaml` resolved values
- Multiple `-e` flags allowed
- Value-only `KEY` (no `=`) reads from host environment (like `docker run -e KEY`)

**CLI definition** (added to both `Run` and `Up` commands):
```rust
/// Set environment variables (KEY=VAL). Overrides .env and clawden.yaml values.
#[arg(short = 'e', long = "env")]
env_vars: Vec<String>,
```

**Security**: Values passed via `-e` are visible in process arguments on the host. This matches Docker's behavior and is acceptable for local development. The CLI must NOT log `-e` values to audit files — only the key names.

### 2. Bot Token and API Key Shortcuts (`--token`, `--api-key`)

The two most common credentials — the channel bot token and the LLM API key — deserve dedicated flags instead of requiring the verbose `-e KEY=VAL` syntax:

```sh
# Zero-config Telegram bot:
clawden run --token 123:abc --channel telegram zeroclaw

# With explicit API key too:
clawden run --api-key sk-... --token 123:abc --channel telegram --provider openai zeroclaw

# Multiple channels use the same --token for all (or -e for per-channel control):
clawden run --token 123:abc --channel telegram --channel discord zeroclaw
```

**`--token` behavior**:
- Sets the token for the channel(s) specified by `--channel`
- Mapped to the correct env var per channel type: `TELEGRAM_BOT_TOKEN`, `DISCORD_BOT_TOKEN`, etc.
- If multiple `--channel` flags are given, `--token` applies to each channel's primary bot token field
- If `--channel` is not specified, do not guess: print a short required-fields summary and ask for explicit channel selection
- For channels that require extra credentials (Slack app token, Signal phone), `--token` is accepted but missing required fields are handled with a friendly guidance message (no panic, no stack trace)

**Additional channel credential flags**:
- `--app-token <TOKEN>`: optional shortcut for channels that require an app token (for example Slack)
- `--phone <E164>`: optional shortcut for channels that require a phone identity (for example Signal)
- These flags only apply to channels selected via `--channel`

### Required-Fields Guidance (UX Contract)

Before startup, `run`/`up` should build and print a compact "Required fields" summary showing:

- Which fields are mandatory for selected channels/providers
- Which values were auto-detected (from CLI flags, `-e`, `--env-file`, `.env`, `clawden.yaml`, vault)
- Which fields are still missing

Example:

```text
Required fields for this run:
    provider: openai
        - OPENAI_API_KEY .......... missing
    channel: telegram
        - TELEGRAM_BOT_TOKEN ...... provided (from --token)

How to continue:
    1) Provide missing fields now: --api-key ..., -e KEY=VAL, or --env-file <path>
    2) Skip credential validation for this run: --allow-missing-credentials
```

This keeps the UX explicit for newcomers while still enabling fast expert workflows.

**`--api-key` behavior**:
- Sets the LLM provider API key for the current run
- Auto-detects which env var to use based on `--provider` or the configured provider: `--provider openai` → `OPENAI_API_KEY`, `--provider anthropic` → `ANTHROPIC_API_KEY`, etc.
- Always sets `CLAWDEN_LLM_API_KEY` and `{RUNTIME}_LLM_API_KEY` (e.g. `ZEROCLAW_LLM_API_KEY`) for compatibility with current direct-mode validation and runtime contracts
- Also sets `CLAWDEN_API_KEY` as a compatibility alias — this is read by some community tools that expect a generic key name; drop this alias if no consumer is identified during implementation
- If no provider can be determined, continue without error and print a short hint: provider-specific env vars will be skipped unless `--provider` is supplied
- If `--provider` is given alongside `--api-key`, both resolve together

**CLI definition**:
```rust
/// Bot/channel token (used with --channel)
#[arg(long)]
token: Option<String>,

/// LLM provider API key (auto-maps to provider env var)
#[arg(long)]
api_key: Option<String>,
```

**Security**: Same as `-e` — values visible in process args on the host, acceptable for local dev. Never logged to audit files.

### 3. Model and Provider Override (`--model`, `--provider`)

Add `--model` and `--provider` flags to `run`:

```sh
clawden run --provider anthropic --model claude-sonnet-4-20250514 zeroclaw
clawden run --model gpt-4o zeroclaw  # uses provider from YAML or infers from model
```

**Behavior**:
- `--provider` overrides `provider` in `clawden.yaml` for this run
- `--model` overrides `model` in `clawden.yaml` for this run
- Both are translated to the runtime's expected env vars/config using the existing config translation pipeline
- If `--provider` is used without a resolved API key, print it as missing in the required-fields summary and offer explicit next actions

**CLI definition**:
```rust
/// LLM provider override (e.g. openai, anthropic, openrouter)
#[arg(long)]
provider: Option<String>,

/// LLM model override (e.g. gpt-4o, claude-sonnet-4-20250514)
#[arg(long)]
model: Option<String>,
```

### 4. System Prompt (`--system-prompt`)

```sh
clawden run --system-prompt "You are a Python tutor" zeroclaw
```

**Behavior**:
- Injected as `CLAWDEN_SYSTEM_PROMPT` env var
- Runtimes that support system prompts read from this env var
- Also written to the generated config file for config-dir runtimes (zeroclaw: `system_prompt` in TOML, picoclaw: `systemPrompt` in JSON)
- If the value starts with `@`, read from file: `--system-prompt @prompt.txt`
- Shell quoting: multi-word prompts must be quoted (`--system-prompt "You are a tutor"`); the `@file` form avoids quoting issues for complex prompts

**CLI definition**:
```rust
/// System prompt for the agent (or @file to read from file)
#[arg(long)]
system_prompt: Option<String>,
```

### 5. Env File Override (`--env-file`)

```sh
clawden run --env-file ./staging.env zeroclaw
```

**Behavior**:
- Loads the specified `.env` file instead of (not in addition to) the auto-detected one
- Allows switching between credential sets (dev, staging, production) without renaming files

**CLI definition**:
```rust
/// Path to .env file (overrides auto-detected .env)
#[arg(long)]
env_file: Option<PathBuf>,
```

### 6. Port Mapping (`-p`)

```sh
clawden run -p 3000:42617 zeroclaw  # map host:3000 → runtime:42617
```

**Behavior**:
- In Direct mode: sets `CLAWDEN_PORT_MAP` env var as a comma-separated list of `HOST:CONTAINER` pairs (e.g. `3000:42617,8080:8080`). Runtimes that support port configuration read this env var. If a runtime ignores `CLAWDEN_PORT_MAP`, the mapping has no effect in Direct mode — this is documented, not an error.
- In Docker mode: passes `-p host:container` to `docker run`
- Common use case: exposing the runtime's HTTP gateway on a known port

**CLI definition**:
```rust
/// Port mapping HOST:CONTAINER (e.g. 3000:42617)
#[arg(short = 'p', long = "port")]
ports: Vec<String>,
```

### 7. Config Show Command (`clawden config show`)

New subcommand to inspect resolved configuration:

```sh
clawden config show              # show all runtimes
clawden config show zeroclaw     # show zeroclaw only
clawden config show --format env # show as env vars instead of native format
```

**Output (default — native format)**:
```
─── zeroclaw (TOML) ───
default_provider = "openrouter"
default_model = "anthropic/claude-sonnet-4-6"

[channels_config.telegram]
bot_token = "***redacted***"

─── Environment Variables ───
CLAWDEN_CHANNELS=telegram
CLAWDEN_TOOLS=git,http
OPENROUTER_API_KEY=***redacted***
```

**Behavior**:
- Loads `clawden.yaml`, resolves env vars, runs config translation, displays the result
- Redacts secrets by default; `--reveal` flag to show actual values (for debugging)
- Shows both the native config file content AND the env vars that would be passed

**Format options**:
- `native` (default): runtime's native config format (TOML for zeroclaw, JSON for picoclaw) plus the env vars that would be passed
- `env`: env-var-only output, one `KEY=VALUE` per line — suitable for piping to `env` or `source`
- `json`: structured JSON with `{ "config": { ... }, "env": { ... } }` — suitable for programmatic consumption

**CLI definition**:
```rust
#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Show resolved configuration for runtimes
    Show {
        /// Runtime to show (shows all if omitted)
        runtime: Option<String>,
        /// Output format: native, env, json
        #[arg(long, default_value = "native")]
        format: String,
        /// Show actual secret values instead of redacting
        #[arg(long, default_value_t = false)]
        reveal: bool,
    },
}
```

### 8. Verbose / Log Level (Global)

Add a global `--verbose` flag and `--log-level` option:

```sh
clawden --verbose run zeroclaw           # show debug output
clawden --log-level trace up             # maximum verbosity
```

**Behavior**:
- `--verbose` / `-v`: enables debug-level logging for ClawDen itself (not runtime logs)
- `--log-level`: sets specific log level (error, warn, info, debug, trace)
- Shows config resolution steps, env var injection, health check results, command construction
- Uses `tracing` crate (already a common Rust pattern with Axum)

**CLI definition** (global args in `Cli` struct):
```rust
/// Enable verbose output
#[arg(short = 'v', long, global = true, default_value_t = false)]
pub verbose: bool,

/// Set log level (error, warn, info, debug, trace)
#[arg(long, global = true)]
pub log_level: Option<String>,
```

**Interaction**: If both `--verbose` and `--log-level` are given, `--log-level` takes precedence. `--verbose` alone is equivalent to `--log-level debug`.

**Constraint**: The `-v` short flag is reserved globally. Future subcommands must not reuse `-v` for other purposes.

### 9. Merge / Precedence Rules

To keep behavior predictable and easy to explain, all credential/config sources follow one precedence order:

1. Explicit CLI key-value overrides (`-e KEY=VAL`)
2. Shortcut CLI flags (`--api-key`, `--token`, `--app-token`, `--phone`, `--provider`, `--model`, `--system-prompt`)
3. Explicit env file from `--env-file`
4. Auto-detected `.env`
5. `clawden.yaml`
6. Provider key vault fallback (when relevant)

**Notes**:
- If both `-e OPENAI_API_KEY=...` and `--api-key ...` are provided, `-e` wins
- `KEY` form of `-e` (without value) imports from host process env at the same highest precedence level as other `-e` entries
- Unknown provider with `--api-key` is non-fatal; generic key vars are still injected for best-effort startup

### 10. Missing-Credential Handling

- Default behavior: if required fields remain missing after resolution, print the required-fields summary and exit with a friendly actionable message (no stack trace)
- Opt-in skip behavior: `--allow-missing-credentials` proceeds even when required fields are missing
- In skip mode, still print the summary and mark fields as missing so users understand likely runtime failure causes
- `--allow-missing-credentials` is supported on both `run` and `up`

### Summary of Flag Changes

**`clawden run` — new flags:**

| Flag | Short | Type | Description |
|---|---|---|---|
| `--env` | `-e` | `Vec<String>` | Inline env vars (KEY=VAL) |
| `--token` | | `Option<String>` | Bot/channel token (with --channel) |
| `--app-token` | | `Option<String>` | Channel app token shortcut |
| `--phone` | | `Option<String>` | Channel phone identity shortcut |
| `--api-key` | | `Option<String>` | LLM provider API key |
| `--allow-missing-credentials` | | `bool` (default `false`) | Proceed even if required credential fields are missing |
| `--provider` | | `Option<String>` | LLM provider override |
| `--model` | | `Option<String>` | LLM model override |
| `--system-prompt` | | `Option<String>` | System prompt (or @file) |
| `--env-file` | | `Option<PathBuf>` | Alternate .env file path |
| `--port` | `-p` | `Vec<String>` | Port mapping HOST:CONTAINER |

**`clawden up` — new flags:**

> `up` intentionally receives fewer flags than `run`. It orchestrates multiple runtimes from `clawden.yaml`, so per-runtime overrides like `--token`, `--provider`, `--model`, `--system-prompt`, and `--port` don't have clear semantics (which runtime would they apply to?). Use `-e` for ad-hoc env var injection, or edit `clawden.yaml` for per-runtime configuration.

| Flag | Short | Type | Description |
|---|---|---|---|
| `--env` | `-e` | `Vec<String>` | Inline env vars |
| `--env-file` | | `Option<PathBuf>` | Alternate .env file path |
| `--allow-missing-credentials` | | `bool` (default `false`) | Proceed even if required credential fields are missing |

**Global — new flags:**

| Flag | Short | Type | Description |
|---|---|---|---|
| `--verbose` | `-v` | `bool` | Debug output |
| `--log-level` | | `Option<String>` | Log level control |

**New command:**

| Command | Description |
|---|---|
| `clawden config show [runtime]` | Show resolved config per runtime |

### Updated Help Output

After implementation, `clawden run -h` must list all new flags in this spec with brief UX-first descriptions and examples for channel credential shortcuts.

## Plan

- [x] Add `-e` / `--env` flag to `Run` command (parse KEY=VAL, inject into env)
- [x] Add `-e` / `--env` flag to `Up` command
- [x] Add `--token` flag to `Run` — map to channel-specific env var based on `--channel`
- [x] Add `--app-token` and `--phone` flags to `Run` for channels with multi-field credentials
- [x] Add `--api-key` flag to `Run` — map to provider-specific env var based on `--provider` or config
- [x] Ensure `--api-key` always sets `CLAWDEN_LLM_API_KEY` and runtime-scoped `*_LLM_API_KEY` (plus optional generic alias)
- [x] Add required-fields summary builder (provider/channel requirements + resolved sources + missing list)
- [x] Add `--allow-missing-credentials` to `Run` and `Up`
- [x] Make missing-required-field flow friendly and actionable (no stack trace panic path)
- [x] Add `--provider` flag to `Run` command
- [x] Add `--model` flag to `Run` command
- [x] Apply `--model` and `--provider` overrides in config translation pipeline
- [x] Add `--system-prompt` flag to `Run` command (with @file support)
- [x] Map `--system-prompt` to env var and config-dir output
- [x] Add `--env-file` flag to `Run` and `Up` commands
- [x] Add `-p` / `--port` flag to `Run` command
- [x] Add `clawden config show` command with runtime resolution and secret redaction
- [x] Add `--verbose` / `-v` global flag
- [x] Add `--log-level` global flag
- [x] Wire verbose/log-level to `tracing` subscriber initialization
- [x] Update audit logging to redact `-e` values (log keys only)
- [x] Add tests for `-e` parsing (KEY=VAL, KEY-only, multiple)
- [x] Add tests for `--model` / `--provider` override in config translation
- [x] Add tests for `config show` output and redaction
- [x] Add tests for precedence matrix (`-e` vs shortcut flags vs env-file vs yaml)
- [x] Add `up` command tests for `-e` and `--env-file`
- [x] Add `-p` / `--port` flag to `Run` tests (Direct mode env var + Docker mode passthrough)
- [x] Add `Config` command variant to the `Commands` enum in `cli.rs`
- [x] Wire `--env-file` into `load_config()` to override `.env` auto-detection path
- [x] Add tests for `config show` format variants (`native`, `env`, `json`)
- [x] Add tests for `--env-file` with `run`
- [x] Add tests for `--verbose` / `--log-level` interaction
- [x] Add tests for duplicate `-e` keys (last occurrence wins)
- [x] Add tests for `--system-prompt` with config-dir runtimes (TOML/JSON output)

## Test

- [x] `clawden run -e FOO=bar zeroclaw` → zeroclaw process receives `FOO=bar` in its environment
- [x] `clawden run -e TELEGRAM_BOT_TOKEN=tok --channel telegram zeroclaw` → works without .env
- [x] `clawden run --token tok --channel telegram zeroclaw` → sets `TELEGRAM_BOT_TOKEN=tok` in runtime env
- [x] `clawden run --api-key sk-... --provider openai zeroclaw` → sets `OPENAI_API_KEY=sk-...` in runtime env
- [x] `clawden run --api-key sk-... zeroclaw` (no provider) → sets `CLAWDEN_LLM_API_KEY` and runtime-scoped key vars without hard error
- [x] `clawden run --token tok --channel telegram --api-key sk-... zeroclaw` → full zero-config run works
- [x] `--token` without `--channel` → prints required-fields guidance and asks for explicit `--channel` (does not guess a default channel)
- [x] `clawden run --channel slack --token xoxb-... --app-token xapp-... zeroclaw` → sets both Slack credentials correctly
- [x] `clawden run --channel signal --token sig-... --phone +15551234567 zeroclaw` → sets Signal token + phone correctly
- [x] `-e` values override `.env` file values for the same key
- [x] `-e OPENAI_API_KEY=override --api-key sk-base --provider openai` → `OPENAI_API_KEY=override` (CLI `-e` wins)
- [x] `clawden run --model gpt-4o --provider openai zeroclaw` → config translation uses overridden values
- [x] `clawden run --system-prompt "test" zeroclaw` → `CLAWDEN_SYSTEM_PROMPT=test` in runtime env
- [x] `clawden run --system-prompt @prompt.txt zeroclaw` → reads prompt from file
- [x] `clawden config show zeroclaw` → displays resolved TOML with redacted secrets
- [x] `clawden config show --reveal zeroclaw` → displays actual secret values
- [x] `--verbose` produces debug output showing config resolution steps
- [x] Audit log for `-e` usage contains key names but not values
- [x] `clawden up -e OPENAI_API_KEY=sk-...` → provider key override is applied for started runtimes
- [x] `clawden up --env-file ./staging.env` → selected env file is used instead of auto-detected `.env`
- [x] Missing provider key with no skip flag → command exits with friendly required-fields summary and remediation options
- [x] Missing provider key with `--allow-missing-credentials` → command proceeds and summary marks key as missing
- [x] `clawden run --env-file ./staging.env zeroclaw` → uses staging.env instead of auto-detected .env
- [x] `clawden run -p 3000:42617 zeroclaw` → Direct mode sets `CLAWDEN_PORT_MAP=3000:42617`; Docker mode passes `-p 3000:42617`
- [x] `clawden config show --format env zeroclaw` → outputs `KEY=VALUE` lines (secrets redacted)
- [x] `clawden config show --format json zeroclaw` → outputs structured JSON
- [x] `--verbose` and `--log-level debug` both produce debug output; `--log-level trace --verbose` uses trace (log-level wins)
- [x] `clawden run --system-prompt "test" zeroclaw` → for config-dir runtimes, value appears in generated TOML/JSON config
- [x] `clawden run -e A=1 -e A=2 zeroclaw` → runtime receives `A=2` (last occurrence wins)

## Notes

### Dependencies

- 033-product-positioning (complete) — `uv run` execution model that this spec extends
- 031-direct-mode-config-injection (in-progress) — config-dir translation pipeline that `--model`/`--provider`/`--system-prompt` must integrate with
- 025-llm-provider-api-key-management (complete) — provider config schema and env var resolution

### Non-Goals

- **Interactive credential prompts at run time** — `clawden init` already handles interactive setup. `clawden run` should be non-interactive; use `-e` or `--env-file` for ad-hoc credentials.
- **Remote secret store integration** — HashiCorp Vault, AWS Secrets Manager, etc. are out of scope. Local `.env` + env vars + `-e` cover the local development use case.
- **Runtime-specific flag translation** — `--model` and `--provider` use the existing config translation pipeline. We don't add runtime-specific flags like `--zeroclaw-verbose`; those go as trailing runtime args.