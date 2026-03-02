---
status: planned
created: 2026-03-02
priority: high
tags:
- cli
- onboarding
- ux
- init
- developer-experience
depends_on:
- 013-config-management
- 023-cli-direct-architecture
- 025-llm-provider-api-key-management
parent: 009-orchestration-platform
created_at: 2026-03-02T01:26:46.553133318Z
updated_at: 2026-03-02T01:26:46.553133318Z
---

# Guided Onboarding & First-Run Experience

> **Status**: planned · **Priority**: high · **Created**: 2026-03-02

## Overview

New users face a steep onboarding cliff: `clawden init` generates a template YAML and a blank `.env`, then leaves the user to manually figure out runtimes, channels, providers, tools, and credentials. There's no interactive guidance, no incremental validation, and no way to verify the setup works until `clawden up` fails at runtime.

This spec introduces a guided, interactive first-run experience that walks users from zero to a working deployment in a single session — validating each step as they go.

## Context

### What Works Today

- `clawden init` scaffolds `clawden.yaml` + `.env` with sensible defaults
- `clawden doctor` checks prerequisites (Docker, Node, Git, etc.)
- `clawden install` downloads runtimes for direct mode
- Config validation catches schema errors at `clawden up` time

### Pain Points

1. **No interactive flow** — users must understand YAML schema, runtime names, channel types, and provider fields before writing config
2. **Validation is post-hoc** — errors only surface when `clawden up` is called; no incremental feedback
3. **Credential testing is unavailable in CLI** — `clawden channels test` only works in dashboard server mode
4. **No "first run" detection** — returning users get the same experience as new users
5. **Docker vs. direct mode confusion** — docs assume Docker; `--no-docker` is buried in flags
6. **No examples or walkthroughs** — generated config has comments but no real-world examples
7. **Manual credential entry** — `.env` must be edited by hand with no guidance on where to obtain tokens

## Design

### 1. Interactive `clawden init` Wizard

Replace the current template-dump with a step-by-step interactive flow (opt-out with `--non-interactive` or `--yes` for CI).

```
$ clawden init

Welcome to ClawDen! Let's set up your project.

Step 1/5 — Execution Mode
  How do you want to run claw runtimes?
  > [1] Docker (recommended — isolated, reproducible)
    [2] Direct install (no Docker required)

Step 2/5 — Runtime Selection
  Which claw runtime(s) do you want to use?
  > [x] ZeroClaw   — general-purpose AI agent
    [ ] OpenClaw   — open interpreter variant
    [ ] PicoClaw   — lightweight/edge agent
    [ ] NanoClaw   — minimal footprint
    (Use space to toggle, enter to confirm)

Step 3/5 — Channel Configuration
  How should your agent(s) communicate?
    [ ] Telegram
    [ ] Discord
    [ ] Slack
    [x] None (API/CLI only for now)

Step 4/5 — LLM Provider
  Detected API keys in environment:
    ✓ OPENAI_API_KEY
    ✗ ANTHROPIC_API_KEY
    ✗ OPENROUTER_API_KEY

  Which LLM provider will you use?
  > [1] OpenAI          (API key detected ✓)
    [2] Anthropic
    [3] Google (Gemini)
    [4] OpenRouter
    [5] Local/self-hosted (OpenAI-compatible)
    [6] Skip for now

  Using detected OPENAI_API_KEY. OK? [Y/n]

Step 5/5 — Tools
  Enable built-in tools:
  > [x] git
    [x] http
    [ ] core-utils
    [ ] python
    [ ] code-tools
    [ ] database

✓ Created clawden.yaml
✓ Created .env (with your API key)
✓ Running doctor check...
  ✓ Docker: available (v27.1.2)
  ✓ Git: available (v2.43.0)
  ✓ Node: available (v22.5.1)

Next steps:
  $ clawden up        — Start your agent
  $ clawden dashboard — Open the web UI
  $ clawden --help    — See all commands
```

### 2. `clawden doctor` Enhancements

Extend `doctor` to validate the full setup, not just prerequisites:

- **Config validation**: Parse and validate `clawden.yaml` without starting runtimes
- **Credential checks**: Test API keys (lightweight ping/auth endpoint) and channel tokens
- **Runtime availability**: Verify runtime binary is installed (direct mode) or Docker image exists
- **Report card format**: Show clear pass/fail/warn for each check with actionable fix suggestions

```
$ clawden doctor

Prerequisites
  ✓ Docker ............... v27.1.2
  ✓ Git .................. v2.43.0
  ✓ Node ................. v22.5.1
  ✓ curl ................. available

Configuration (clawden.yaml)
  ✓ Schema valid
  ✓ Runtime "zeroclaw" defined
  ✗ Provider "openai" — API key not set
    → Set OPENAI_API_KEY in .env or run: clawden init --reconfigure

Runtimes
  ✓ zeroclaw ............. installed (v0.4.2)

Channels
  ⚠ No channels configured (agent will be CLI/API only)

Overall: 1 error, 1 warning — fix errors before running `clawden up`
```

### 3. First-Run Detection

Detect when a user is running ClawDen for the first time — all three conditions must be true: no `~/.clawden/` directory, no `clawden.yaml` in CWD, **and** no installed runtimes. If any installed runtimes exist, fall through to the current `up` behavior (run installed runtimes) to avoid interrupting existing workflows. Users can suppress the prompt with `--no-prompt`:

```
$ clawden up
No clawden.yaml found. Would you like to set up a new project?
  [Y] Run setup wizard
  [n] Exit
```

### 4. `clawden init --reconfigure`

Allow re-running the wizard on an existing project to add channels, change providers, or update credentials without overwriting existing config. Merges selections into the existing `clawden.yaml`.

### 5. Quick-Start Templates

Offer named templates for common setups:

```
$ clawden init --template telegram-bot
$ clawden init --template discord-bot
$ clawden init --template api-only
$ clawden init --template multi-runtime
```

Each template pre-fills the YAML with a known-good configuration and annotates which `.env` variables need to be set.

## Plan

- [ ] Auto-detect provider env vars (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `OPENROUTER_API_KEY`, `GEMINI_API_KEY` / `GOOGLE_API_KEY`, etc.) and surface detected keys in the wizard. For Google/Gemini, check both `GEMINI_API_KEY` (primary, per Google quickstart docs) and `GOOGLE_API_KEY` (alias, takes precedence per SDK if both set)
- [ ] Add `dialoguer` (or similar) crate for interactive terminal prompts
- [ ] Refactor `init` command to support interactive wizard flow
- [ ] Add `--non-interactive` / `--yes` flag to preserve CI-friendly behavior
- [ ] Preserve existing `--runtime`, `--multi`, and `--force` flags: `--runtime` sets the default selection in the wizard, `--multi` selects the multi-runtime template path, `--force` allows overwriting existing config
- [ ] Add `--template <name>` flag with bundled templates
- [ ] Add `--reconfigure` flag for additive config updates
- [ ] Extend `doctor` command with config validation section
- [ ] Extend `doctor` command with credential testing (lightweight auth pings)
- [ ] Extend `doctor` command with runtime availability checks
- [ ] Add first-run detection to `up` and `run` commands
- [ ] Add quick-start templates (telegram-bot, discord-bot, api-only, multi-runtime)
- [ ] Write integration tests for wizard flow (simulated stdin)
- [ ] Update README with getting-started walkthrough referencing `clawden init`

## Test

- [ ] `clawden init` in empty directory launches interactive wizard and produces valid `clawden.yaml` + `.env`
- [ ] `clawden init --yes` produces config non-interactively (CI-safe)
- [ ] `clawden init --template telegram-bot` generates correct template with placeholder env vars
- [ ] `clawden init --reconfigure` on existing project merges without data loss
- [ ] `clawden doctor` reports config errors and credential issues with actionable messages
- [ ] `clawden up` with no config triggers first-run prompt
- [ ] Wizard masks credential input (no plaintext API keys on screen)

## Notes

### Dependencies

- 013-config-management (complete) — YAML schema and validation
- 023-cli-direct-architecture (complete) — direct mode runtime resolution
- 025-llm-provider-api-key-management (in-progress) — provider config section

### Security Rules

- The wizard must never write resolved secret values to disk. Generated `.env` files should contain placeholder references (e.g., `OPENAI_API_KEY=`) rather than copying detected values from the host environment.
- Credential input during the wizard must be masked (no plaintext API keys on screen).

### Non-Goals

- GUI/web-based onboarding (the dashboard already exists for ongoing management)
- Account creation or cloud sign-up flows
- Auto-provisioning of external API keys or bot tokens
