---
status: complete
created: 2026-03-03
priority: high
tags:
- positioning
- product
- ux
- strategy
created_at: 2026-03-03T08:49:22.936640Z
updated_at: 2026-03-04T01:35:03.298362720Z
transitions:
- status: in-progress
  at: 2026-03-04T01:12:13.164677517Z
---
# ClawDen Product Positioning — UX Shell, Runtime Manager, SDK Platform

## Overview

ClawDen has evolved beyond "orchestration platform" into three distinct, complementary product roles. This spec clarifies ClawDen's identity and establishes positioning language to guide architecture decisions, documentation, and marketing.

### Problem

The current positioning — "unified orchestration platform" / "Kubernetes of claw agents" — is technically accurate but creates two issues:

1. **Over-indexes on infra.** It frames ClawDen as ops tooling for fleet management, when most users are solo developers or hobbyists running 1–2 runtimes locally. The CLI-Direct architecture (023) already acknowledged this by eliminating the mandatory server.
2. **Under-sells the UX/DX value.** ClawDen's biggest value isn't orchestration — it's that a user can do `npx clawden run zeroclaw` and everything just works.

### Unique Selling Point

**ClawDen simplifies the UX/DX for xxxclaw deployment and usage.**

Every claw runtime (OpenClaw, ZeroClaw, PicoClaw, etc.) has its own config format, deployment model, dependency chain, and startup ritual. ClawDen collapses all of that into a single command. The USP is not "orchestration" — it's that ClawDen makes claw runtimes **accessible to anyone**, regardless of infra expertise.

### The `uv run` Model — One Command to Rule Them All

The gold-standard UX is **one command from zero to running**:

```bash
npx clawden run zeroclaw
```

This single command should:
1. **Install ClawDen** — `npx` handles this via the npm package (already works)
2. **Install the runtime** — `ensure_installed_runtime()` auto-installs if missing (already works)
3. **Prompt for credentials** — if no API key is configured, interactively ask for it (new)
4. **Start the runtime** — launch and stream logs (already works)

This is the `uv run` / `bunx` philosophy: resolve all dependencies on the fly, ask for what's needed, never make the user run prerequisite commands.

#### What already works today

| Step                        | Status | Implementation                                                      |
| --------------------------- | ------ | ------------------------------------------------------------------- |
| `npx` entry point           | Done   | `npm/clawden/package.json` bin + postinstall                        |
| Auto-install runtime        | Done   | `ensure_installed_runtime()` in `util.rs` — installs on first `run` |
| Version pinning from config | Done   | Reads `clawden.yaml` for pinned versions                            |
| Start + log streaming       | Done   | `ProcessManager::start_direct_with_env_and_project()`               |
| Provider key vault          | Done   | `providers set-key` stores encrypted keys                           |

#### What's missing: interactive credential flow during `run`

When a user runs `npx clawden run zeroclaw` with no config and no API key, the experience should be:

```
$ npx clawden run zeroclaw

Runtime 'zeroclaw' not installed. Installing latest...
Installed zeroclaw@0.8.1

No LLM provider API key found.
Which provider? [openai/anthropic/custom]: openai
Enter your OpenAI API key: sk-••••••••
✓ Key validated and saved to vault.

Starting zeroclaw...
```

Key design decisions for the credential flow:
- Only prompt when running interactively (detect `stdin.is_terminal()` — already used in `set_provider_key`)
- In non-interactive/CI mode, fail with clear error: "Missing API key. Set OPENAI_API_KEY or run `clawden providers set-key openai`"
- Validate the key before saving (reuse `test_provider_endpoint()`)
- Store in encrypted vault (reuse `store_provider_key_in_vault()`)
- Remember for subsequent runs — prompt only once ever

### The Three Roles

#### 1. UX Shell (primary)

ClawDen is the **unified command-line and dashboard experience** for the xxxclaw ecosystem. Like how `gh` wraps Git+GitHub into a cohesive workflow, ClawDen wraps heterogeneous claw runtimes behind a single, opinionated interface.

**Analogy:** `uv` / `gh` CLI / Docker Desktop

Key UX surfaces:
- CLI commands: `run`, `up`, `ps`, `stop`, `channels`, `config`
- Guided onboarding: `clawden init` → interactive runtime selection
- Dashboard: real-time monitoring, log streaming, channel management
- Config generation: `clawden config gen` → unified TOML regardless of runtime

What this means for decisions:
- CLI ergonomics and error messages are first-class concerns
- Default behaviors should "just work" for the single-runtime case
- Power-user features (fleet, swarm) are discoverable but not required

#### 2. Runtime Manager (secondary)

ClawDen manages claw runtime **installations, versions, and updates** — exactly like `nvm` manages Node.js versions or `rustup` manages Rust toolchains.

**Analogy:** nvm / rustup / pyenv

Key capabilities:
- `clawden install zeroclaw` — download/install a runtime
- `clawden install zeroclaw@0.5.2` — pin a specific version (planned)
- `clawden install --upgrade` — update installed runtimes (spec 028)
- `clawden install --list` — show installed runtimes
- `clawden install --outdated` — check for available updates
- `clawden uninstall zeroclaw` — remove a runtime
- Auto-install on `run` — like `uv run`, resolves automatically

**Note on naming:** `install` is preferred over `pull` because:
- Matches user mental model — you "install" software, you "pull" images
- Consistent with `npm install`, `brew install`, `apt install`
- `pull` implies Docker/Git semantics that don't apply to direct-install mode

#### 3. SDK Platform (tertiary)

ClawDen provides the **cross-runtime development kit** for building skills/plugins that work across claw variants.

**Analogy:** Terraform Provider SDK / VS Code Extension API

Key capabilities:
- `@clawden/sdk` — TypeScript SDK with `defineSkill()` API
- `clawden skill create` / `clawden skill test` — scaffolding and cross-runtime testing
- Adapter abstraction — skills don't know which runtime they're running on
- (Future) Skill marketplace

### Positioning Statement

> **ClawDen** simplifies xxxclaw deployment and usage. One command to install, configure, and run any claw runtime — plus a cross-runtime SDK for building skills that work everywhere.

### Elevator Pitches by Role

| Role            | One-liner                                                                           |
| --------------- | ----------------------------------------------------------------------------------- |
| UX Shell        | "`uv run` for claw agents — one command to install, configure, and run any runtime" |
| Runtime Manager | "nvm for claw runtimes — install, switch, and update with one command"              |
| SDK Platform    | "Build once, run on any claw — cross-runtime skills with TypeScript"                |

## Design

### Persona Alignment

| Persona          | Primary role used          | Entry point                               |
| ---------------- | -------------------------- | ----------------------------------------- |
| Hobbyist/student | UX Shell                   | `npx clawden run zeroclaw`                |
| Solo developer   | UX Shell + Runtime Manager | `npx clawden init && clawden up`          |
| Skill author     | SDK Platform               | `clawden skill create my-skill`           |
| Team/enterprise  | All three + fleet features | `clawden dashboard` + fleet orchestration |

### Impact on Architecture

This positioning reinforces several existing architectural decisions:
- **CLI-Direct (023)**: Correct — UX Shell should work without server overhead
- **Guided onboarding (026)**: Correct — first-run experience is critical for UX Shell role
- **Runtime pull/update (028)**: Correct — this is core Runtime Manager functionality
- **SDK package (015, 019)**: Correct — SDK is a distinct distribution concern

Potential gaps this positioning reveals:
- **Runtime version pinning**: `clawden pull zeroclaw@0.5.2` not yet implemented
- **Offline catalog**: Pre-pulled runtimes should work without network access
- **Persona-aware docs**: README and docs should speak to the persona, not the architecture
- **`clawden doctor`**: A diagnostic command to verify runtime health, versions, and config — common in UX-first tools

### `uv run`-Style Transparent Execution

The `clawden run` command should adopt the **`uv run` execution model** — the user feels like they are running the runtime directly, while ClawDen transparently manages installation, environment, config injection, and lifecycle behind the scenes.

#### Analogy

| Tool      | Command                                        | What really happens                                                                                                              |
| --------- | ---------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `uv`      | `uv run ruff check --fix .`                    | uv ensures ruff is installed in the venv, then execs `ruff check --fix .`                                                        |
| `clawden` | `clawden run zeroclaw --verbose --model gpt-4` | clawden ensures zeroclaw is installed, injects config/channels/tools via env vars, then execs `zeroclaw --verbose --model gpt-4` |

#### Argument Separation

ClawDen flags go **before** the runtime name; everything **after** the runtime name belongs to the runtime:

```
clawden run [clawden-flags...] <runtime> [runtime-args...]
```

Examples:
```sh
# All args after "zeroclaw" are zeroclaw's own args
clawden run zeroclaw --verbose --model gpt-4

# ClawDen flags (--channel, --with, -d) come before the runtime name
clawden run --channel telegram --with web-search zeroclaw --verbose

# Detach + runtime args
clawden run -d --channel discord openclaw --port 3000 --debug

# Bare run — no clawden flags, no runtime args
clawden run zeroclaw
```

This eliminates the current `--` separator requirement (`clawden run zeroclaw -- --verbose`), matching the ergonomics users expect from `uv run`, `npx`, `cargo run`, and `go run`.

#### What ClawDen Does Transparently

When the user runs `clawden run zeroclaw --verbose`:

1. **Auto-install** — if zeroclaw is not installed (or a pinned version is missing), install it first (like `uv run` auto-creates the venv)
2. **Config translation** — load `clawden.yaml`, translate unified config into the runtime's native format (see below)
3. **Exec the runtime** — pass `--verbose` (and any other trailing args) directly to the zeroclaw binary
4. **Lifecycle management** — stream logs, handle Ctrl+C gracefully, cleanup on `--rm`

The user never needs to know about config translation, env vars, or installation paths.

#### Config Translation: `clawden.yaml` → Runtime-Native Format

The core of the `uv run` analogy is that `uv` manages the venv so you don't have to; ClawDen manages config translation so you don't have to. One `clawden.yaml` is automatically translated into each runtime's native configuration format.

**Two delivery mechanisms:**

| Mechanism                   | Runtimes                                         | How                                                                                                                   |
| --------------------------- | ------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------- |
| **Config-dir** (file-based) | zeroclaw, picoclaw, nullclaw, openfang (Phase 2) | Generates `config.toml` or `config.json` in `~/.clawden/configs/<project_hash>/<runtime>/`, passes `--config-dir` arg |
| **Env-var only**            | openclaw, nanoclaw                               | No config file support; everything via `CLAWDEN_*` and `<RUNTIME>_*` env vars                                         |

**Config format per runtime:**

| Runtime  | Language   | Config Format | `--config-dir` | Env Prefix   |
| -------- | ---------- | ------------- | -------------- | ------------ |
| zeroclaw | Rust       | TOML          | Yes            | `ZEROCLAW_*` |
| picoclaw | Go         | JSON          | Yes            | (none)       |
| nullclaw | —          | TOML          | Yes            | TBD          |
| openfang | Rust       | TOML          | Yes (Phase 2)  | `OPENFANG_*` |
| openclaw | TypeScript | JSON5         | No (env-only)  | (none)       |
| nanoclaw | TypeScript | Code/inline   | No (env-only)  | `NANOCLAW_*` |

**Field mapping example (zeroclaw TOML):**

| clawden.yaml                         | →   | zeroclaw config.toml                            |
| ------------------------------------ | --- | ----------------------------------------------- |
| `provider: openrouter`               |     | `default_provider = "openrouter"`               |
| `model: anthropic/claude-sonnet-4-6` |     | `default_model = "anthropic/claude-sonnet-4-6"` |
| `providers.openrouter.api_key`       |     | `reliability.api_keys[].key`                    |
| `channels.telegram.token`            |     | `channels_config.telegram.bot_token`            |
| `channels.discord.guild`             |     | `channels_config.discord.guild_id`              |
| `channels.slack.bot_token`           |     | `channels_config.slack.bot_token`               |
| `config.*` (arbitrary)               |     | Merged into TOML root                           |

**Translation pipeline (all invisible to the user):**

```
clawden.yaml
    │
    ├─ load_config()          → parse + validate + resolve $ENV_VAR refs
    ├─ build_runtime_env_vars()  → map provider/channel creds to env vars
    ├─ generate_config_dir()  → route to runtime-specific file generator:
    │   ├─ zeroclaw/nullclaw/openfang → generate_toml_config()  → config.toml
    │   └─ picoclaw                   → generate_picoclaw_config() → config.json
    ├─ inject_config_dir_arg()   → append --config-dir <path> to start args
    ├─ validate_direct_runtime_config() → pre-start credential checks
    └─ exec runtime with env vars + args
```

**What gets validated before exec:**
- Provider API key is non-empty when a provider is configured
- Channel tokens are non-empty for each enabled channel (type-specific requirements: Slack needs both `bot_token` + `app_token`, Signal needs both `phone` + `token`)
- Actionable error messages on failure: `Error: provider 'openrouter' is configured for runtime 'zeroclaw' but API key is missing. → Set it in .env / clawden.yaml or run: clawden providers set openrouter`

**Project isolation:** Config dirs use `<project_hash>` to prevent cross-project pollution — each `clawden.yaml` in a different directory gets its own config namespace under `~/.clawden/configs/`.

#### Complete Example

User creates `clawden.yaml`:
```yaml
runtime: zeroclaw
provider: openrouter
model: anthropic/claude-sonnet-4-6
providers:
  openrouter:
    api_key: $OPENROUTER_API_KEY
channels:
  support:
    type: telegram
    token: $TELEGRAM_BOT_TOKEN
```

User runs:
```sh
clawden run zeroclaw --verbose
```

What ClawDen does (invisibly):
1. Loads `clawden.yaml`, resolves `$OPENROUTER_API_KEY` and `$TELEGRAM_BOT_TOKEN` from `.env`/shell
2. Generates `~/.clawden/configs/a3f1c2/zeroclaw/config.toml`:
   ```toml
   default_provider = "openrouter"
   default_model = "anthropic/claude-sonnet-4-6"

   [reliability]
   [[reliability.api_keys]]
   provider = "openrouter"
   key = "sk-or-..."

   [channels_config.telegram]
   bot_token = "123456:ABC..."
   ```
3. Validates provider key and channel token are non-empty
4. Execs: `zeroclaw --config-dir ~/.clawden/configs/a3f1c2/zeroclaw --verbose`
5. Sets env vars: `CLAWDEN_CHANNELS=telegram`, `CLAWDEN_LLM_PROVIDER=openrouter`, etc.
6. Streams logs, handles Ctrl+C

The user sees only: their agent starts and works. The config translation is completely invisible.

#### `--help` Passthrough

`clawden run zeroclaw --help` should show **zeroclaw's** help, not clawden's. This is the strongest signal that `clawden run` is a transparent exec wrapper. ClawDen's own run flags are documented via `clawden run --help` (without a runtime name).

#### Implementation: Clap Changes

The current `Run` command uses `#[arg(last = true)]` for extra args, requiring `--`. The `uv run` model requires:

```rust
/// Run a single runtime
#[command(trailing_var_arg = true)]
Run {
    /// Channels to connect (clawden flag — must come before runtime name)
    #[arg(long)]
    channel: Vec<String>,
    /// Tools to enable (clawden flag)
    #[arg(long = "with")]
    tools: Option<String>,
    /// Remove one-off state after exit
    #[arg(long, default_value_t = false)]
    rm: bool,
    /// Run in background and return immediately
    #[arg(short = 'd', long, default_value_t = false)]
    detach: bool,
    /// Restart on failure policy
    #[arg(long)]
    restart: Option<String>,
    /// Runtime name followed by its arguments
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    runtime_and_args: Vec<String>,
},
```

The first positional value in `runtime_and_args` is the runtime name; the rest are forwarded to it verbatim. This is the same pattern `cargo run` uses for `cargo run --release -- args` but without requiring `--`.

### Documentation & Messaging Guidance

- README should lead with: `npx clawden run zeroclaw` — one command, nothing else needed
- Hero section: "Get a claw agent running in 10 seconds"
- Error messages should suggest next steps, not expose internal state
- `--help` text should use plain language ("Run a claw agent" not "Invoke lifecycle management")

## Plan

- [x] Update README.md to reflect UX Shell-first positioning
- [x] Audit CLI `--help` text for plain-language clarity
- [x] Add `clawden doctor` diagnostic command
- [x] Implement runtime version pinning (`@version` syntax)
- [x] Write persona-aligned documentation sections
- [x] Review AGENTS.md description to align with new positioning
- [x] Implement `uv run`-style transparent arg passing in `Run` command (remove `--` separator requirement)
- [x] Ensure `clawden run <runtime> --help` passes through to runtime's own help
- [x] Document config translation pipeline (clawden.yaml → runtime-native format) for each supported runtime
- [x] Add config translation coverage for openfang `--config-dir` (Phase 2)

## Test

- [x] README communicates value proposition in first 3 lines
- [x] `clawden --help` output is understandable by someone who has never seen ClawDen
- [x] Each persona can complete their entry-point workflow in under 60 seconds
- [x] Positioning language is consistent across CLI, dashboard, docs, and package descriptions
- [x] `clawden run zeroclaw --verbose` works without `--` separator (trailing args forwarded)
- [x] `clawden run zeroclaw --help` shows zeroclaw's help output, not clawden's
- [x] Config translation produces valid native config for each runtime (zeroclaw TOML, picoclaw JSON)
- [x] Pre-exec validation catches missing credentials with actionable error messages
- [x] Project isolation: two projects with different clawden.yaml get independent config dirs