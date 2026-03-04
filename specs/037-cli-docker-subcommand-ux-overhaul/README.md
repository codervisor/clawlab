---
status: planned
created: 2026-03-04
priority: high
tags:
- cli
- ux
- docker
- developer-experience
- overhaul
created_at: 2026-03-04T14:08:09.114409Z
updated_at: 2026-03-04T14:08:09.114409Z
---

# CLI Docker Subcommand & UX Overhaul — Dedicated Docker Surface, Allowed-Users Shortcut & Arg Declutter

> **Status**: planned · **Priority**: high · **Created**: 2026-03-04

## Overview

The `clawden run` and `clawden up` commands have accumulated Docker-specific flags (`--no-docker`, `-p/--port`) alongside Direct-mode credential shortcuts (`--token`, `--api-key`, `--provider`, etc.), creating a crowded help page and a confusing mental model. Meanwhile, Docker-aware workflows (image pull, container inspect, volume/network management, explicit image-based runs) have no first-class CLI surface — users must drop down to raw `docker` commands, losing ClawDen's config-injection and adapter benefits.

Additionally, `clawden run` is missing an `--allowed-users` shortcut. Telegram's `allowed_users` is a critical security field (empty = deny all), yet it can only be set via `clawden.yaml` or `-e`, making quick ad-hoc runs harder than necessary.

### Problems

| #   | Problem                                                                                                                                                                                                              | Impact                                                                    |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| 1   | **`clawden run -h` is 25+ flags** — Docker flags, credential flags, channel flags, and runtime flags are all interleaved                                                                                             | New users are overwhelmed; help output buries the most common options     |
| 2   | **No `clawden docker` subcommand** — Docker-specific operations (pull, images, ps, exec, build) require switching to raw `docker` CLI, losing ClawDen's adapter metadata and config injection                        | Breaks the "single tool" promise; expert users need two CLIs side by side |
| 3   | **Missing `--allowed-users` on `run`** — Telegram's allowed_users (the most security-critical channel field) can only be set via YAML/env. In contrast, `--token`, `--app-token`, `--phone` all have dedicated flags | Zero-config quickstarts for Telegram are insecure or require file edits   |
| 4   | **`--no-docker` is a negation flag** — Cognitive overhead; users must reason about double negatives ("don't not use Docker"). Docker bypass is opt-out rather than opt-in                                            | Better solved by making Docker explicit via `clawden docker run`          |
| 5   | **`-p/--port` is Docker-only but lives on `run`** — Port mapping has no meaning in Direct mode, yet it's always shown in help                                                                                        | Misleading; users try `-p` in Direct mode and nothing happens             |

### User Stories

- **Quick Telegram bot test**: `clawden run --token 123:abc --allowed-users 42 --channel telegram zeroclaw` — one command, secure by default
- **Docker power user**: `clawden docker run -p 8080:3000 --channel telegram zeroclaw` — Docker explicitly chosen, port mapping makes sense
- **Image management**: `clawden docker pull zeroclaw:latest`, `clawden docker images` — manage claw runtime images without switching to raw Docker
- **Clean ad-hoc run**: `clawden run --api-key sk-... --token abc zeroclaw` — no Docker flags cluttering the help page
- **Container inspection**: `clawden docker ps`, `clawden docker logs zeroclaw` — see running containers through ClawDen's lens

### Why Now

Spec 033 positioned `clawden run` as the "`uv run` for claw agents" — lean, transparent, Direct-mode-first. But the command has since accumulated Docker-specific flags that undermine this positioning. Meanwhile, users who *do* want Docker have no dedicated surface. This spec resolves both tensions by giving Docker its own home.

## Analysis — Current State

**`clawden run` (22 flags)** — Docker-specific flags to **move** to `docker run`: `-p/--port`, `--rm`, `--restart`. Flag to **remove**: `--no-docker` (Direct is already default). Flag to **add**: `--allowed-users`. All credential/channel/tool flags **stay**.

**`clawden up` (9 flags)** — `--no-docker` **removed** (use `mode: direct` in YAML or `clawden docker up` for explicit Docker). All other flags **stay**.

## Design

### 1. New `clawden docker` Subcommand Group

Introduce `clawden docker` as a subcommand group that mirrors Docker CLI conventions but operates within ClawDen's adapter/config ecosystem. This is **not** a raw Docker passthrough — it uses ClawDen's adapter traits, config injection, and channel resolution, but forces Docker execution mode.

```
clawden docker <SUBCOMMAND>

Subcommands:
  run       Run a claw runtime in Docker (one-off)
  up        Start runtimes from clawden.yaml in Docker mode
  ps        List running claw containers
  images    List locally available claw runtime images
  pull      Pull/update a claw runtime image
  logs      View container logs
  exec      Execute command in a running claw container
  stop      Stop a running claw container
  rm        Remove a stopped claw container
  build     Build a custom claw runtime image
```

#### `clawden docker run` — Docker-explicit one-off execution

```
clawden docker run [OPTIONS] [RUNTIME_AND_ARGS]...

Options:
  -p, --port <PORTS>                   Port mapping (HOST:CONTAINER). Multiple allowed
  -v, --volume <VOLUMES>               Volume mount (HOST:CONTAINER). Multiple allowed
      --rm                             Remove container after exit
  -d, --detach                         Run in background
      --restart <RESTART>              Restart policy (no|on-failure|always|unless-stopped)
      --name <NAME>                    Container name override
      --network <NETWORK>              Docker network to join
      --channel <CHANNEL>              Channels to connect
  -e, --env <ENV_VARS>                 Set environment variables (KEY=VAL)
      --env-file <ENV_FILE>            Override auto-detected .env file
      --provider <PROVIDER>            Override provider
      --model <MODEL>                  Override model
      --token <TOKEN>                  Channel token shortcut
      --api-key <API_KEY>              LLM API key shortcut
      --app-token <APP_TOKEN>          Channel app token (e.g. Slack)
      --phone <PHONE>                  Channel phone (e.g. Signal)
      --system-prompt <SYSTEM_PROMPT>  Override system prompt
      --allowed-users <USERS>          Comma-separated user allowlist (e.g. Telegram)
      --with <TOOLS>                   Tools to enable
      --allow-missing-credentials      Skip credential validation
      --image <IMAGE>                  Override Docker image (default: inferred from runtime adapter)
```

**Key behaviors:**
- Always uses Docker execution mode (no `--no-docker` flag needed)
- Inherits all credential shortcut flags from `run` (for config injection into containers)
- Adds Docker-specific flags: `-p`, `-v`, `--rm`, `--restart`, `--name`, `--network`, `--image`
- ClawDen config injection still applies (adapter generates config.toml, passes env vars)

#### `clawden docker up` — Multi-runtime Docker orchestration

```
clawden docker up [OPTIONS] [RUNTIMES]...

Options:
  -e, --env <ENV_VARS>             Set environment variables
      --env-file <ENV_FILE>        Override .env file
  -d, --detach                     Run in background
      --no-log-prefix              Disable runtime name prefixes
      --timeout <TIMEOUT>          Shutdown timeout [default: 10]
      --allow-missing-credentials  Skip credential validation
      --build                      Rebuild images before starting
      --force-recreate             Recreate containers even if config unchanged
```

**Behavior:** Like `clawden up` but always Docker mode. Config from `clawden.yaml` is still used, but execution is forced to Docker regardless of `mode:` setting.

#### `clawden docker ps` — List running claw containers

```
clawden docker ps [OPTIONS]

Options:
  -a, --all    Show all containers (including stopped)
  --format     Output format (table|json)
```

Filters to show only ClawDen-managed containers (by label `clawden.managed=true`). Shows: NAME, RUNTIME, IMAGE, STATUS, PORTS, CREATED.

#### `clawden docker images` — List claw runtime images

```
clawden docker images [OPTIONS] [RUNTIME]

Options:
  -a, --all    Show all images (including intermediate)
  --format     Output format (table|json)
```

Filters to claw runtime images. Shows: RUNTIME, TAG, IMAGE ID, SIZE, CREATED.

#### `clawden docker pull` — Pull/update runtime image

```
clawden docker pull [OPTIONS] <RUNTIME>

Options:
  --tag <TAG>    Specific image tag (default: latest/configured)
```

Uses the runtime adapter to resolve the correct image name, then pulls.

#### `clawden docker exec` — Exec into running container

```
clawden docker exec [OPTIONS] <RUNTIME> [COMMAND]...

Options:
  -it           Interactive TTY (default if no command)
  --user <USER> User to exec as
```

#### `clawden docker stop|rm|logs|build`

Standard lifecycle commands scoped to ClawDen-managed containers. `logs` supports `-f/--follow` and `--tail N`. `build` accepts `--runtime`, `--tag`, `-f`, `--no-cache`.

### 2. Simplified `clawden run` (Direct-mode focus)

After extracting Docker-specific flags, `clawden run` becomes leaner and focused on its Direct-mode identity:

```
clawden run [OPTIONS] [RUNTIME_AND_ARGS]...

Arguments:
  [RUNTIME_AND_ARGS]...  Runtime name followed by runtime args

Options:
      --channel <CHANNEL>              Channels to connect
  -e, --env <ENV_VARS>                 Set environment variables (KEY=VAL)
      --env-file <ENV_FILE>            Override .env file
      --provider <PROVIDER>            Override provider
      --model <MODEL>                  Override model
      --token <TOKEN>                  Channel token shortcut
      --api-key <API_KEY>              LLM API key shortcut
      --app-token <APP_TOKEN>          Channel app token (e.g. Slack)
      --phone <PHONE>                  Channel phone (e.g. Signal)
      --system-prompt <SYSTEM_PROMPT>  Override system prompt. Prefix with @ to load from file
      --allowed-users <USERS>          Comma-separated user allowlist (e.g. Telegram user IDs)
      --with <TOOLS>                   Tools to enable
      --allow-missing-credentials      Skip credential validation
  -d, --detach                         Run in background
  -v, --verbose
      --log-level <LOG_LEVEL>
  -h, --help                           Print help
```

**Changes from current:**
- **Removed**: `--no-docker`, `-p/--port`, `--rm`, `--restart` (moved to `docker run`)
- **Added**: `--allowed-users`
- **Net result**: 18 → 16 flags (and more internally coherent)

### 3. `--allowed-users` Flag

Add an `--allowed-users` shortcut flag to both `clawden run` and `clawden docker run`:

```sh
# Quick secure Telegram bot:
clawden run --token 123:abc --allowed-users 42,67890 --channel telegram zeroclaw

# Multiple users, space-separated alternative:
clawden run --token 123:abc --allowed-users "42 67890" --channel telegram zeroclaw
```

**Behavior:**
- Accepts a comma-separated (or space-separated) list of user IDs
- Applied to the channel(s) selected by `--channel` that support user allowlisting (currently: Telegram)
- Translated to the correct config field per channel type (Telegram → `allowed_users` array in config.toml)
- When used with channels that don't support allowlisting, silently ignored with a debug-level log
- If `--channel telegram` is used WITHOUT `--allowed-users`, behavior is unchanged (empty array = deny all, matching current safe default)

**CLI definition:**
```rust
/// Comma-separated user allowlist (e.g. Telegram user IDs)
#[arg(long)]
allowed_users: Option<String>,
```

**Implementation:** In `apply_shortcut_env_overrides()`, parse the value and inject it into the channel config:
```rust
if let Some(users_str) = &opts.allowed_users {
    let users: Vec<String> = users_str
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    // Inject into channel config for config_gen
}
```

### 4. `--no-docker` Flag Removal

The `--no-docker` global flag is removed entirely once `clawden docker` exists. It is no longer needed:

| Scenario                   | Before                             | After                                                   |
| -------------------------- | ---------------------------------- | ------------------------------------------------------- |
| Force Direct mode on `run` | `clawden run --no-docker zeroclaw` | `clawden run zeroclaw` (already default)                |
| Force Direct mode on `up`  | `clawden up --no-docker`           | `clawden up` (Direct default) or `mode: direct` in YAML |
| Force Docker mode          | `mode: docker` in YAML             | `clawden docker run zeroclaw` or `mode: docker` in YAML |

### 5. `clawden up` Auto-Mode Behavior (Preserved)

`clawden up` retains Auto mode detection (prefer Docker if available, fall back to Direct). This is intentional — `up` is the multi-runtime orchestration command where Docker provides real value (isolation, networking). Users who need Direct mode set `mode: direct` in YAML.

`clawden docker up` is available for users who want to **explicitly force Docker mode** regardless of config.

### 6. Architecture — Command Registration

```
Cli
├── init
├── run           ← simplified, Direct-focused
├── up            ← Auto mode (preserved)
├── down
├── start / stop / restart
├── install / uninstall
├── ps
├── logs
├── config
│   ├── show
│   └── env
├── channels
├── providers
├── tools
│   ├── list
│   └── info
├── dashboard
├── doctor
└── docker        ← NEW subcommand group
    ├── run       ← Docker-explicit one-off
    ├── up        ← Docker-explicit orchestration
    ├── ps        ← List claw containers
    ├── images    ← List claw images
    ├── pull      ← Pull runtime image
    ├── exec      ← Exec into container
    ├── stop      ← Stop container
    ├── rm        ← Remove container
    ├── logs      ← Container logs
    └── build     ← Build custom image
```

### 7. Adapter Integration for Docker Commands

The `clawden docker` subcommands should use the existing adapter trait system (`ClawAdapter`) rather than shelling out to `docker` directly. This ensures:

- Correct image names from adapter metadata (e.g., `ghcr.io/xxxxxxxxxxxx/zeroclaw:latest`)
- Config injection via `generate_config_dir()` + volume mounts
- Consistent env var resolution via `build_runtime_env_vars()`
- Runtime-specific Docker flags from adapter `docker_options()` trait method

For commands that have no adapter-level abstraction (e.g., `exec`, `build`), the CLI can shell out to `docker` with appropriate label filters (`clawden.managed=true`, `clawden.runtime={name}`).

## Plan

### Step 1: `clawden docker` subcommand (MVP)
- [ ] Add `Docker` subcommand group to clap CLI
- [ ] Implement `docker run` with full Docker-specific flags (`-p`, `-v`, `--rm`, `--restart`, `--name`, `--network`, `--image`) plus all credential/channel flags
- [ ] Implement `docker ps` — list ClawDen-managed containers by label
- [ ] Implement `docker images` — list claw runtime images
- [ ] Implement `docker pull` — pull runtime image via adapter metadata

### Step 2: Simplify existing commands
- [ ] Remove `--no-docker`, `-p/--port`, `--rm`, `--restart` from `clawden run`
- [ ] Remove `--no-docker` from `clawden up`
- [ ] Add `--allowed-users` to `clawden run`
- [ ] Update all integration tests

### Step 3: Extended Docker commands
- [ ] Implement `docker up` (force-Docker orchestration)
- [ ] Implement `docker stop` / `docker rm`
- [ ] Implement `docker logs` with `-f` and `--tail`
- [ ] Implement `docker exec`
- [ ] Implement `docker build`

### Step 4: Cleanup
- [ ] Update documentation & README
- [ ] Update dashboard RuntimeCatalog if needed

## Breaking Changes

| Change                           | Notes                                 |
| -------------------------------- | ------------------------------------- |
| `--no-docker` removed from `run` | Use `clawden run` (Direct is default) |
| `--no-docker` removed from `up`  | Use `mode: direct` in clawden.yaml    |
| `-p/--port` removed from `run`   | Use `clawden docker run -p`           |
| `--rm` removed from `run`        | Use `clawden docker run --rm`         |
| `--restart` removed from `run`   | Use `clawden docker run --restart`    |
| `--allowed-users` added to `run` | Additive                              |
| `clawden docker` added           | Additive                              |

## Non-Goals

- **Not a raw Docker passthrough**: `clawden docker` is not `docker` — it operates through ClawDen's adapter layer with config injection. Users needing raw Docker access should use `docker` directly.
- **Not replacing `docker compose`**: `clawden docker up` uses ClawDen's multi-runtime system, not compose files.
- **Not changing `up` default mode**: `clawden up` keeps Auto mode (prefer Docker if available). Only `clawden run` is Direct-default.


