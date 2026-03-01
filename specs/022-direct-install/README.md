---
status: in-progress
created: 2026-03-01
priority: high
tags:
- install
- deployment
- native
- cli
- direct-install
depends_on:
- 023-cli-direct-architecture
parent: 009-orchestration-platform
created_at: 2026-03-01T02:44:52.520937Z
updated_at: 2026-03-01T06:31:16.990765Z
transitions:
- status: in-progress
  at: 2026-03-01T06:31:16.990765Z
---
# Direct Install — Docker-Free Deployment

## Overview

Provide a **native install path** for ClawDen so users who don't have Docker (or don't want it) can deploy and run claw runtimes directly on their host machine. A single `clawden install` command downloads runtimes, sets up tools, and manages processes — no containers, no Docker dependency.

### Problem

The current deployment story (`clawden up`) requires Docker. Many target users — hobbyists, students, Raspberry Pi users, WSL-without-Docker setups, shared hosting — don't have Docker installed and shouldn't need to learn it just to run an AI agent on Telegram. Users need an alternative that works without Docker.

> **Prerequisite**: This spec assumes the CLI-direct architecture from spec 023 is in place — the CLI calls `clawden-core` directly instead of going through an HTTP server. This spec adds Docker-free runtime installation and execution on top of that foundation.

### Goal

`clawden run zeroclaw --channel telegram` works whether Docker is installed or not. If Docker is present, ClawDen uses Docker by default for sandboxing and safer isolation. If Docker is not available (or `--no-docker` is explicitly set), ClawDen runs the runtime directly on the host.

## Design

### Runtime Resolution Order

When the user runs `clawden run <runtime>`, ClawDen checks in order:

1. **Docker available?** → Use container by default (security-first behavior from spec 017)
2. **Runtime binary installed locally?** → Run directly on host
3. **Neither?** → Prompt: `Runtime 'zeroclaw' not installed. Run 'clawden install zeroclaw' to install it.`

Users can force direct mode with `--no-docker` or set `CLAWDEN_NO_DOCKER=1` to always skip Docker.

### `clawden install` Command

Downloads and sets up runtimes natively on the host:

```bash
# Install a specific runtime
clawden install zeroclaw
clawden install openclaw
clawden install picoclaw

# Install all Phase 1 runtimes
clawden install --all

# Install a specific version
clawden install zeroclaw@0.1.7

# List installed runtimes
clawden install --list

# Uninstall
clawden uninstall zeroclaw
```

### Install Directory Layout

```
~/.clawden/
├── config.toml              # Global ClawDen preferences
├── runtimes/
│   ├── zeroclaw/
│   │   ├── 0.1.7/
│   │   │   └── zeroclaw     # Binary
│   │   └── current -> 0.1.7 # Symlink to active version
│   ├── picoclaw/
│   │   ├── latest/
│   │   │   └── picoclaw
│   │   └── current -> latest
│   ├── openclaw/
│   │   └── current/         # npm global install (node_modules)
│   └── nanoclaw/
│       └── current/         # Git clone + pnpm install
├── tools/
│   ├── git/setup.sh         # Copied from repo or downloaded
│   └── http/setup.sh
└── cache/
    └── downloads/           # Cached tarballs / archives
```

### Download Sources

Same upstream sources as the Docker image — no new infrastructure needed:

| Runtime  | Source                                     | Install Method                                |
| -------- | ------------------------------------------ | --------------------------------------------- |
| ZeroClaw | GitHub Releases (`zeroclaw-labs/zeroclaw`) | Download binary for platform                  |
| PicoClaw | GitHub Releases (`picoclaw-labs/picoclaw`) | Download binary for platform                  |
| OpenClaw | npm registry                               | `npm install -g openclaw` into managed prefix |
| NanoClaw | GitHub repo (`qwibitai/nanoclaw`)          | `git clone` + `pnpm install`                  |

### Platform Detection

Binary runtimes need the correct platform artifact. ClawDen detects:

- **OS**: `linux`, `darwin` (Phase 1). `windows` deferred to Phase 4 — symlinks, signal handling, and PID management require platform-specific implementations.
- **Arch**: `x64` (`x86_64`), `arm64` (`aarch64`)

Maps to upstream release naming conventions per runtime (e.g., `zeroclaw-0.1.7-linux-x86_64.tar.gz`).

### Download Validation (Simple Baseline)

Use a simple, practical validation policy that avoids heavy key or manifest management:

- **Transport**: Only download from trusted HTTPS upstreams already used by the Docker flow.
- **Binary runtimes (GitHub Releases)**: Validate expected artifact name/pattern for platform, ensure non-empty archive, and ensure extraction yields the expected executable path.
- **npm runtimes (OpenClaw)**: Rely on npm's default package integrity and TLS checks via standard `npm install`.
- **Git-cloned runtimes (NanoClaw)**: Clone from the canonical repository URL and validate expected project files before marking install complete.
- **Optional checksum path**: If a runtime publishes `SHA256SUMS`, verify it and fail on mismatch; if not present, continue with baseline validation.

If validation fails:
```
[clawden] ERROR: Download validation failed for zeroclaw-0.1.7-linux-x86_64.tar.gz
         Reason: archive is missing expected runtime binary
         The artifact may be corrupted or incompatible. Aborting install.
```

### Process Management (Direct Mode)

Without Docker, ClawDen manages runtime processes directly:

```bash
clawden up                    # Starts runtimes as background processes
clawden ps                    # Shows PIDs, uptime, status
clawden stop                  # Sends SIGTERM, waits, SIGKILL fallback
clawden logs zeroclaw         # Tails log file
```

Implementation:
- **PID files**: `~/.clawden/run/<runtime>.pid`
- **Log files**: `~/.clawden/logs/<runtime>.log` (rotated, max 10MB × 5)
- **Stdout/stderr**: Redirected to log files in background mode
- **Health checks**: Same `GET /health` polling as Docker mode
- **Crash restart**: Optional `--restart=on-failure` with backoff (1s → 2s → 4s → max 30s)
- **Audit logging**: All lifecycle events (install, start, stop, crash, restart, uninstall) are recorded to `~/.clawden/logs/audit.log` with timestamp, runtime name, event type, and outcome. This satisfies the project-wide requirement that all lifecycle events must be audit-logged (see AGENTS.md).

### Config Translation (Reuse)

The same `clawden.yaml` config works for both Docker and direct mode. The credential mapping logic (env var translation per runtime) is implemented in `clawden-core`. The `ProcessManager` (from spec 023) handles both Docker containers and native processes through its `ExecutionMode` enum. Config translation code paths in `clawden-core` are shared across both modes — no duplication.

### Tool Setup (Direct Mode)

Tools in direct mode run the same `setup.sh` scripts but on the host instead of inside a container:

- **`git`**: Verify `git` is installed on host, warn if missing
- **`http`**: Verify `curl`/`wget` available, warn if missing
- **`browser`**: Check for Chromium/Chrome, offer to install Playwright
- **`gui`**: Not supported in direct mode (requires X server config — out of scope)

Tools that can't be satisfied show a clear message:
```
[clawden] Tool 'git' requires git to be installed on your system.
         Install it with: brew install git (macOS) / apt install git (Debian/Ubuntu)
```

### Install Locking & Atomicity

Concurrent `clawden install` invocations must not corrupt `~/.clawden/`:

- **File-based lock**: Acquire an exclusive lock on `~/.clawden/.install.lock` before writing to `runtimes/` or `cache/`. Use `flock` (Unix) advisory locking.
- **Atomic directory swap**: Downloads go to a temporary directory (`~/.clawden/runtimes/<runtime>/.<version>.tmp`). After checksum verification, the directory is renamed atomically to its final path. If the process is interrupted, the temp directory is cleaned up on next install.
- **Cache writes**: Cache archive writes use a temp filename + rename pattern to prevent partial files from being used.

### Environment Isolation

Direct mode runs runtimes with a controlled environment:
- Working directory: `./workspace` (or `--workdir` override)
- Environment variables: Only those specified in `clawden.yaml` + runtime defaults
- No PATH pollution: Runtime binaries are invoked by absolute path

### CLI Changes Summary

| Command                       | New / Changed | Description                                     |
| ----------------------------- | ------------- | ----------------------------------------------- |
| `clawden install <runtime>`   | **New**       | Download + install a runtime natively           |
| `clawden install --list`      | **New**       | List installed runtimes + versions              |
| `clawden install --all`       | **New**       | Install all Phase 1 runtimes                    |
| `clawden uninstall <runtime>` | **New**       | Remove installed runtime                        |
| `clawden run`                 | **Changed**   | Falls back to direct mode if Docker unavailable |
| `clawden up`                  | **Changed**   | Supports direct mode process management         |
| `clawden ps`                  | **Changed**   | Shows PID info in direct mode                   |
| `clawden stop`                | **Changed**   | SIGTERM/SIGKILL in direct mode                  |
| `clawden logs`                | **New**       | Tail runtime log files (direct mode)            |
| `clawden run --no-docker`     | **New flag**  | Force direct mode                               |

## Plan

### Phase 1: Core Direct Install
- [x] Implement `clawden install <runtime>` — platform detection + GitHub Release download for binary runtimes
- [x] Implement `clawden install` for Node.js runtimes (OpenClaw via npm, NanoClaw via git clone)
- [x] Implement `~/.clawden/runtimes/` directory layout with version management + symlinks
- [x] Implement download cache (`~/.clawden/cache/`) to avoid re-downloading
- [x] Implement `clawden install --list` and `clawden uninstall`
- [x] Add Docker detection in `clawden run` — fall back to direct mode when Docker unavailable
- [x] Implement `--no-docker` flag and `CLAWDEN_NO_DOCKER` env var
- [x] Implement baseline download validation (artifact pattern, extraction sanity, optional checksum verification when available)
- [x] Implement install locking (`~/.clawden/.install.lock`) and atomic directory swap
- [x] Implement audit logging for install/uninstall lifecycle events

### Phase 2: Process Management
- [x] Implement direct-mode process spawning (background, PID files, log redirection)
- [x] Implement `clawden ps` for direct mode (PID, uptime, port, status)
- [x] Implement `clawden stop` for direct mode (SIGTERM → SIGKILL)
- [x] Implement `clawden logs` for direct mode (tail log files)
- [x] Implement health check polling for direct-mode runtimes
- [x] Implement crash restart with exponential backoff (`--restart=on-failure`)
- [x] Implement audit logging for start/stop/crash/restart lifecycle events

### Phase 3: Tool Verification & Polish
- [x] Implement host tool verification (git, curl, browser checks) with actionable install hints
- [x] Implement `clawden install --all` for bulk install
- [x] Add `clawden doctor` command — checks system prerequisites, installed runtimes, connectivity
- [x] Add upgrade support: `clawden install zeroclaw@latest` re-downloads if newer version available
- [x] Documentation: direct install quickstart guide

### Phase 4: Windows Support (Deferred)
- [ ] Replace symlinks with junction points or `.current` marker files on Windows
- [ ] Replace SIGTERM/SIGKILL with `TerminateProcess` / `ctrl_c_event` on Windows
- [ ] Replace `flock` with Windows named mutex for install locking
- [ ] Add Windows platform detection and artifact naming support
- [ ] Windows-specific tool install hints (winget, choco, scoop)

## Test

- [x] `clawden install zeroclaw` downloads correct binary for current platform to `~/.clawden/runtimes/`
- [x] `clawden install openclaw` runs `npm install` into managed prefix successfully
- [x] `clawden install --list` shows installed runtimes with versions
- [x] `clawden uninstall zeroclaw` removes runtime and cleans up symlinks
- [x] `clawden run zeroclaw` uses direct mode when Docker is not installed
- [x] `clawden run zeroclaw --no-docker` forces direct mode even when Docker is available
- [x] `clawden.yaml` config works identically in direct mode and Docker mode
- [x] `clawden up` starts runtimes as background processes with PID files in direct mode
- [x] `clawden ps` shows correct process status (running, stopped, crashed) in direct mode
- [x] `clawden stop` cleanly shuts down runtime processes
- [x] `clawden logs zeroclaw` streams runtime logs from log files
- [x] Health check detects crashed runtimes and reports status accurately
- [x] Missing tool on host produces a helpful error message with install instructions
- [x] `clawden doctor` reports system readiness accurately
- [x] Corrupted or incomplete archive is rejected with clear error message
- [x] Valid artifact passes baseline validation and install completes successfully
- [x] Concurrent `clawden install zeroclaw` invocations don't corrupt `~/.clawden/`
- [x] Interrupted install leaves no partial directory in `~/.clawden/runtimes/`
- [x] Install, uninstall, start, stop, crash, restart events appear in `~/.clawden/logs/audit.log`
- [x] `clawden run --no-docker` bypasses HTTP server dependency and spawns runtime directly via `clawden-core`

## Notes

- This spec complements spec 017 (Docker) — not a replacement. Docker remains the recommended path for production and multi-runtime deployments. Direct install is the easy on-ramp for single-runtime hobbyist use.
- Download sources are identical to what the Dockerfile uses — no new build infra needed.
- The `clawden` npm package already installs a native CLI binary (spec 019). This spec extends that CLI with `install` / `uninstall` subcommands.
- Node.js runtimes (OpenClaw, NanoClaw) require Node.js on the host. `clawden install openclaw` should check for Node.js and give a clear error if missing.
- Version pinning in `~/.clawden/runtimes/<runtime>/<version>/` allows multiple versions side-by-side, but `current` symlink determines which one `clawden run` uses.
- Crash restart with backoff prevents CPU burn if a runtime is misconfigured.
- `clawden doctor` is inspired by `flutter doctor` — checks everything in one command.
- Future consideration: systemd unit / launchd plist generation for `clawden up` as a system service (out of scope for now).
- Windows support is deferred to Phase 4. The design relies on Unix symlinks, `flock`, and POSIX signals — all of which need platform-specific alternatives on Windows.
- OpenFang was considered for the download sources table but is excluded because it is not in the current `ClawRuntime` enum or adapter registry. It can be added in a future spec if needed.

- Implemented direct-install command surface in `clawden-cli`: `install`, `install --all`, `install --list`, `uninstall`, `logs`, `doctor`.
- Added `RuntimeInstaller` and `ProcessManager` in `clawden-core` with lock file, atomic temp-dir swap, runtime symlink management, PID/log files, and audit log writes.
- `clawden run`/`up` now perform docker detection and direct fallback with `--no-docker` and `CLAWDEN_NO_DOCKER` support.

- Replaced installer stubs with real host install paths in `clawden-core`: ZeroClaw via GitHub release asset discovery + tar extraction, PicoClaw via GitHub artifact + 7z extraction, OpenClaw via npm managed prefix, NanoClaw via git clone + pnpm install.
- Verified with live commands: `install --list`, `install zeroclaw`, `uninstall zeroclaw`, `doctor`, and audit log inspection for install/start/stop/uninstall events.
- `install openclaw` completed successfully in live validation and remains sensitive to npm network/dependency resolution time in constrained environments.
- Added direct-mode restart supervision for `--restart=on-failure` with exponential backoff and crash/restart audit records.
- Added direct-mode health status surface in `clawden ps` with runtime-specific env overrides (`CLAWDEN_HEALTH_URL_<RUNTIME>` / `CLAWDEN_HEALTH_PORT_<RUNTIME>`).
- Hardened install locking to recover from stale `.install.lock` files by validating lock owner PID before failing.
