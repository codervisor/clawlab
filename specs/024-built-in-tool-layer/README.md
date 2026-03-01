---
status: in-progress
created: 2026-03-01
priority: high
tags:
- docker
- tools
- sandbox
- security
- runtime
- infra
depends_on:
- 017-docker-runtime-images
parent: 009-orchestration-platform
created_at: 2026-03-01T08:33:31.613247Z
updated_at: 2026-03-01T08:33:31.613247Z
---

# Built-in Tool Layer — Container Environment & Tool Management

## Overview

ClawDen's Docker runtime embeds **built-in tools** — pre-packaged environment capabilities that any claw runtime can use regardless of its native plugin system. Tools provide the OS-level substrate (Python, browsers, compilers, sandboxes) that runtime plugins depend on to actually function. This spec defines the tool categories, manifest format, tiering system, activation pipeline, security model, and management CLI that let the tool layer scale from 4 tools today to dozens without becoming unmanageable.

## Context

### What Exists Today

Spec 017 established the tool concept with four entries:

| Tool      | Status      | What it does                         |
| --------- | ----------- | ------------------------------------ |
| `git`     | Implemented | Git + SSH client                     |
| `http`    | Implemented | curl, wget                           |
| `browser` | Placeholder | Headless Chromium (empty directory)  |
| `gui`     | Placeholder | Xvfb + VNC desktop (empty directory) |

Each tool is a `setup.sh` script sourced by the entrypoint. Users opt in via `tools: [git, http]` in `clawden.yaml` or `--with git,http` on the CLI.

### Why This Isn't Enough

1. **Runtime plugins need OS support.** An OpenClaw "code execution" plugin is useless without Python in the container. A ZeroClaw "web search" skill needs curl or a browser engine at the OS level. The runtime's native tool system defines *what the LLM can invoke*; the container's built-in tools define *what's actually executable*.

2. **No structure for growth.** Four tools with just `setup.sh` is fine. Twenty tools needs manifests, tiers, dependency tracking, size budgets, and a CLI to manage them.

3. **No security boundary.** Agents that execute arbitrary code or shell commands need sandboxing. Without it, a misbehaving agent can exfiltrate data, saturate the network, or corrupt the workspace.

4. **Phase 2 tools are undefined.** `browser` and `gui` are listed in 017's plan but have no design — no install method, no size budget, no activation sequence, no security model.

### Design Principle

**Tools are environment layers, not runtime features.** A tool adds capabilities to the container that any runtime can use. Tools are orthogonal to the claw runtime — ZeroClaw and OpenClaw both get the same Python, the same browser, the same sandbox.

## Design

### Tool Tiers

Tools are organized into tiers based on size, install cost, and universality:

| Tier          | Install Strategy                                                     | Image Impact            | Description                                 |
| ------------- | -------------------------------------------------------------------- | ----------------------- | ------------------------------------------- |
| **Core**      | Baked into base image, always present                                | Minimal (~50MB total)   | Fundamental utilities every agent needs     |
| **Standard**  | Pre-installed in default image, activated on request                 | Moderate (~200MB total) | Common capabilities most agents use         |
| **Extended**  | Available in use-case image variants or installed at container start | Heavy (200–450MB each)  | Specialized capabilities, split by use case |
| **Community** | User-provided, mounted or installed at start                         | Varies                  | Custom tools via `~/.clawden/tools/`        |

Extended tools ship as **use-case image variants**:

| Image Tag       | Includes                                 | ~Size  | Use Case                                                                      |
| --------------- | ---------------------------------------- | ------ | ----------------------------------------------------------------------------- |
| `:latest`       | Core + Standard                          | ~650MB | General-purpose agent runtime                                                 |
| `:browser`      | `:latest` + headless Chromium/Playwright | ~1.1GB | LLM web browsing — search, scrape, fill forms                                 |
| `:computer-use` | `:browser` + Xvfb/VNC/noVNC/fluxbox      | ~1.3GB | Full computer-use agent — GUI interaction, visual browser, desktop automation |
| `:full`         | `:computer-use` + compiler toolchain     | ~1.6GB | Advanced users — native compilation, build from source                        |

Any extended tool can also be **lazy-installed** into `:latest` at first start (volume-cached), so users are not forced to pick a variant upfront.

### Tool Catalog

#### Core Tools (always available)

| Tool         | Components                          | ~Size | What It Gives the Agent                                  |
| ------------ | ----------------------------------- | ----- | -------------------------------------------------------- |
| `git`        | git, openssh-client                 | 30MB  | Clone repos, commit, push, manage code                   |
| `http`       | curl, wget, ca-certificates         | 5MB   | Make HTTP requests, download files                       |
| `core-utils` | jq, yq, tree, file, zip/unzip, gzip | 15MB  | Structured data manipulation, file inspection, archiving |

#### Standard Tools (pre-installed, opt-in)

| Tool         | Components                          | ~Size | What It Gives the Agent                                       |
| ------------ | ----------------------------------- | ----- | ------------------------------------------------------------- |
| `python`     | Python 3.12, pip, venv              | 120MB | Run scripts, data processing, ML, the universal glue language |
| `code-tools` | ripgrep, fd-find, bat, hexyl        | 25MB  | Fast code search, syntax-highlighted file viewing             |
| `database`   | SQLite 3 CLI + shared libs          | 5MB   | Local persistent storage, structured queries                  |
| `network`    | netcat, socat, dnsutils, traceroute | 15MB  | Network diagnostics, TCP/UDP testing, DNS queries             |
| `sandbox`    | bubblewrap (bwrap)                  | 5MB   | Isolated code execution with resource limits                  |

#### Extended Tools (use-case image variants or on-demand)

| Tool       | Components                        | ~Size | Image Variant   | What It Gives the Agent                                           |
| ---------- | --------------------------------- | ----- | --------------- | ----------------------------------------------------------------- |
| `browser`  | Chromium 130+, Playwright 1.49+   | 450MB | `:browser`      | Headless web browsing — LLM-driven search, scrape, form fill      |
| `gui`      | Xvfb, x11vnc, noVNC, fluxbox      | 200MB | `:computer-use` | Full virtual desktop for computer-use agents (requires `browser`) |
| `compiler` | gcc, g++, make, cmake, pkg-config | 250MB | `:full`         | Compile C/C++ code, build native extensions (advanced users)      |

### Tool Manifest Format

Every tool has a `manifest.toml` alongside its `setup.sh`:

```toml
# /opt/clawden/tools/python/manifest.toml
[tool]
name = "python"
description = "Python 3.12 runtime with pip and venv"
version = "3.12"
tier = "standard"             # core | standard | extended
phase = 2

[size]
installed = "120MB"
download = "40MB"

[dependencies]
requires = []                 # other tools needed first
conflicts = []                # tools that clash

[install]
method = "apt"                # apt | binary | script
packages = ["python3", "python3-pip", "python3-venv"]

[capabilities]
provides = ["python3", "pip3"]                    # commands made available
env = { CLAWDEN_PYTHON = "/usr/bin/python3" }     # env vars set on activation
```

`setup.sh` reads the manifest but remains the executable entry point. The manifest is machine-readable metadata; `setup.sh` is the imperative activation logic.

### Activation Pipeline

The entrypoint processes tools in order:

```
1. Read TOOLS env var (comma-separated list)
2. For each tool:
   a. Load manifest.toml → check dependencies (fail fast if unmet)
   b. Source setup.sh → validate installed, configure PATH/env
   c. Write entry to /run/clawden/tools.json (capabilities file)
3. Export CLAWDEN_TOOLS="git,http,python" (summary env var)
4. Launch runtime
```

#### Capabilities File

After tool activation, the entrypoint writes `/run/clawden/tools.json`:

```json
{
  "activated": ["git", "http", "python", "sandbox"],
  "tools": {
    "git": { "version": "2.43.0", "bin": "/usr/bin/git" },
    "http": { "version": "8.5.0", "bin": "/usr/bin/curl" },
    "python": { "version": "3.12.3", "bin": "/usr/bin/python3" },
    "sandbox": { "version": "0.9.0", "bin": "/usr/bin/bwrap" }
  }
}
```

Runtimes can read this file to discover what's available — e.g., an OpenClaw plugin can check whether `python` is activated before offering a "run code" action to the LLM.

### Installation vs. Activation

Key distinction:

- **Installation**: Getting binaries into the image. Happens at Docker build time (core/standard) or container start (extended/community). Costs disk space and build time.
- **Activation**: Making them available to the runtime. Happens at entrypoint time — sourcing `setup.sh`, setting PATH, exporting env vars. Costs milliseconds.

In **Docker mode**: Core + Standard tools are pre-installed. `setup.sh` only activates. Extended tools can be lazily installed at first start (cached via volume).

In **Direct mode** (spec 022): `setup.sh` checks if installed, installs if missing using the host package manager (brew, apt), or prints a clear message if it can't:
```
[clawden/tools/browser] Chromium not found. Install with:
  brew install chromium (macOS)
  apt install chromium-browser (Debian/Ubuntu)
```

### Security: The `sandbox` Tool

The `sandbox` tool is the security cornerstone for agents executing arbitrary code. It uses bubblewrap (`bwrap`) to create isolated execution environments:

```bash
# Agent wants to run untrusted code
clawden-sandbox exec --timeout 30s --memory 256m -- python3 script.py
```

Under the hood:
```bash
bwrap \
  --ro-bind / / \             # read-only root filesystem
  --tmpfs /tmp \              # writable temp only
  --bind "$WORKSPACE" "$WORKSPACE" \  # writable workspace
  --dev /dev \
  --unshare-net \             # no network by default
  --unshare-pid \             # isolated process tree
  --die-with-parent \         # cleanup on exit
  -- python3 script.py
```

**Security properties:**
- **Network isolation**: No outbound access by default. Opt-in with `--allow-network`
- **Filesystem isolation**: Read-only root, writable only in /tmp and workspace
- **Resource caps**: `--timeout`, `--memory`, and `--cpu` are enforced by the wrapper with cgroup/ulimit controls, not just command-line flags
- **Process isolation**: Cannot see or signal host/other-agent processes
- **Auto-cleanup**: Temp files and processes killed on exit or timeout

In multi-tenant mode, `clawden-sandbox` is **fail-closed**: if required kernel/cgroup primitives are unavailable, execution is denied with a clear error instead of running unsandboxed.

The wrapper script (`/usr/local/bin/clawden-sandbox`) is installed by the sandbox tool's `setup.sh`. Runtimes call it instead of executing code directly when untrusted input is involved.

### Browser Tool Design

`browser` provides headless Chromium and Playwright, then starts a persistent browser server:

```bash
# setup.sh starts browser server in background
# install step is skipped when binaries are already present
# for variant images, Chromium is pre-baked at image build time
# for lazy-install mode, install occurs once and is volume-cached
npx playwright run-server --port 3100 &

# Capabilities
export CLAWDEN_BROWSER_WS="ws://localhost:3100"
```

Runtimes connect via WebSocket endpoint. Multiple runtime instances share one browser server. `setup.sh` must be idempotent: if the server is already healthy on port 3100, it reuses the existing process and does not spawn duplicates.

**Size mitigation**: Chromium is ~400MB. For the `:latest` image, it's not included. Users who need it either:
1. Use `clawden-runtime:browser` (or `:computer-use` / `:full` which include it)
2. Or use a volume-cached lazy install on `:latest` (first start downloads, subsequent starts skip)

### GUI Tool Design

`gui` provides a virtual desktop accessible via VNC/noVNC for full computer-use agents. It always requires `browser` — computer-use is a superset that includes visual browser interaction (point-and-click, screenshots) plus arbitrary GUI app control.

Because this may run in remote multi-tenant deployments, GUI access is secure-by-default:
- No unauthenticated VNC (`-nopw`) is allowed.
- VNC/noVNC bind to localhost by default; external access is only via an authenticated gateway.
- noVNC sessions require short-lived, per-session tokens tied to runtime identity.
- TLS termination is required at the gateway boundary for remote access.

```bash
# setup.sh (runs after browser setup.sh)
Xvfb :99 -screen 0 1280x1024x24 &
export DISPLAY=:99
x11vnc -display :99 -forever -rfbauth /run/clawden/x11vnc.pass -localhost -rfbport 5900 &
# noVNC web client
/opt/noVNC/utils/novnc_proxy --vnc localhost:5900 --listen 127.0.0.1:6080 &

export CLAWDEN_DISPLAY=":99"
export CLAWDEN_VNC_PORT="5900"
export CLAWDEN_NOVNC_PORT="6080"
```

The `:computer-use` image variant includes both `browser` and `gui`. Agents get headless browsing *and* a full desktop — the LLM can choose between Playwright API calls (fast, structured) or visual mouse/keyboard interaction (when DOM access isn't enough).

### The `core-utils` Tool

New addition — always-available utilities that are too small to justify separate tools but universally useful:

```bash
# setup.sh
apt-get install -y -qq jq yq tree file zip unzip gzip 2>/dev/null || true
echo "[clawden/tools/core-utils] jq $(jq --version), tree, file, zip ready"
```

These are baked into the base image. `setup.sh` is a validation-only no-op.

### Tool Management CLI

As the tool catalog grows, users need tooling to manage tools:

```bash
# Discovery
clawden tools list                     # all available tools (with tier, size, status)
clawden tools list --installed         # only installed tools
clawden tools info python              # detailed info for one tool

# Management (mainly for direct-install mode)
clawden tools install browser          # install an extended tool
clawden tools remove browser           # remove a tool
clawden tools update                   # update all installed tools

# Authoring
clawden tools create my-tool           # scaffold a custom tool in ~/.clawden/tools/
```

Example output:
```
$ clawden tools list
TOOL         TIER       SIZE    STATUS      DESCRIPTION
git          core       30MB    activated   Git + SSH client
http         core        5MB    activated   curl, wget
core-utils   core       15MB    installed   jq, yq, tree, file, zip
python       standard  120MB    installed   Python 3.12 + pip + venv
code-tools   standard   25MB    installed   ripgrep, fd, bat
database     standard    5MB    installed   SQLite 3
network      standard   15MB    installed   netcat, dnsutils, traceroute
sandbox      standard    5MB    installed   bubblewrap execution sandbox
browser      extended  450MB    available   Headless Chromium + Playwright
gui          extended  200MB    available   Xvfb + VNC/noVNC desktop
compiler     extended  250MB    available   gcc, g++, make, cmake
```

### Scaling Strategy

- Flat directory under `/opt/clawden/tools/`, each with `manifest.toml` + `setup.sh`.
- Resolve dependencies with a topological sort before activation.
- Add `/opt/clawden/tools/registry.toml` for fast discovery.
- Future: support namespaced community tools via remote registry.

### Image Layering

Docker layers are ordered so each use-case variant extends the previous one:

```
Layer 1: debian:bookworm-slim + Node.js 22           ~200MB  ─┐
Layer 2: Core tools (git, http, core-utils)            ~50MB   │ :latest
Layer 3: Standard tools (python, code-tools, etc.)    ~170MB   │
Layer 4: Claw runtimes (zeroclaw, openclaw, etc.)      ~100MB ─┘
Layer 5: Chromium + Playwright                         ~450MB ─── :browser
Layer 6: Xvfb + x11vnc + noVNC + fluxbox               ~200MB ─── :computer-use
Layer 7: gcc, g++, make, cmake                         ~250MB ─── :full
```

Each variant is an additive layer on the previous. Layers 2–3 change infrequently, so most rebuilds only touch Layer 4. The `:browser` → `:computer-use` → `:full` chain shares all lower layers, so pulling `:computer-use` when you already have `:browser` only downloads ~200MB.

## Plan

### Phase 1: Foundation
- [x] Add `manifest.toml` to existing `git` and `http` tools
- [x] Create `core-utils` tool (jq, yq, tree, file, zip)
- [x] Update `entrypoint.sh` to read manifests, resolve dependencies, write capabilities file
- [x] Add `core-utils` to Dockerfile base image layer
- [x] Add `CLAWDEN_TOOLS` summary env var export

### Phase 2: Standard Tools
- [ ] Create `python` tool (Python 3.12, pip, venv)
- [ ] Create `code-tools` tool (ripgrep, fd-find, bat)
- [ ] Create `database` tool (SQLite 3)
- [ ] Create `network` tool (netcat, socat, dnsutils)
- [ ] Create `sandbox` tool (bubblewrap + `clawden-sandbox` wrapper)
- [ ] Add standard tools to Dockerfile as separate layer
- [ ] Implement `clawden tools list` and `clawden tools info` CLI commands

### Phase 3: Extended Tools & Use-Case Variants
- [ ] Create `browser` tool (Chromium + Playwright + persistent server)
- [ ] Build `:browser` image variant (`:latest` + browser layer)
- [ ] Create `gui` tool (Xvfb + x11vnc + noVNC + fluxbox, depends on `browser`)
- [ ] Build `:computer-use` image variant (`:browser` + gui layer)
- [ ] Create `compiler` tool (gcc, g++, make, cmake)
- [ ] Build `:full` image variant (`:computer-use` + compiler layer)
- [ ] Implement lazy install for extended tools in `:latest` image (volume-cached)

### Phase 4: Ecosystem
- [ ] Implement `clawden tools install/remove/update` for direct-install mode
- [ ] Implement `clawden tools create` scaffolding for custom tools
- [ ] Add `registry.toml` for tool discovery
- [ ] Document custom tool authoring guide

## Test

- [ ] `clawden run zeroclaw --with git,python` — Python 3 available inside runtime
- [ ] `clawden run openclaw --with browser` — Playwright connects to headless Chromium
- [ ] `tools.json` capabilities file written with correct versions after activation
- [ ] Tool with unmet dependency fails fast at entrypoint: `gui` without `browser` errors clearly
- [ ] `sandbox` isolates execution — sandboxed process cannot access network, cannot read files outside workspace
- [ ] `sandbox` enforces memory/CPU/time limits; attempts to exceed limits are terminated and audited
- [ ] Multi-tenant fail-closed behavior: if sandbox primitives are unavailable, untrusted execution is denied
- [ ] `clawden tools list` shows all tools with tier, size, and status
- [ ] Extended tool lazy install works — first start installs to volume, second start skips
- [ ] Custom tool in `~/.clawden/tools/my-tool/` with valid `manifest.toml` is discovered and activatable
- [ ] Image sizes: `:latest` < 700MB, `:browser` < 1.2GB, `:computer-use` < 1.4GB, `:full` < 1.7GB
- [ ] GUI security defaults: VNC/noVNC are localhost-only by default, require auth token flow, and are reachable remotely only through authenticated TLS gateway

## Notes

- `core-utils` replaces ad-hoc installs scattered across runtime configs — single source for common CLI tools
- `sandbox` is the highest-priority standard tool — without it, agents running arbitrary code are a security liability
- `browser` uses Playwright server mode rather than per-request Chromium startup
- Node.js is already in the base image for OpenClaw/NanoClaw
- `gui` always requires `browser` — computer-use is a superset of browser-use. The `:computer-use` image includes both.
- `compiler` targets advanced users only (custom native extensions, research). Most agents never need it.
- Image variants form a strict chain: `:latest` ⊂ `:browser` ⊂ `:computer-use` ⊂ `:full` — each adds one layer
- Direct-install mode (spec 022) uses the same manifests and `setup.sh` scripts — the only difference is install method (apt in container vs. host package manager)
- `yq` refers to the Go-based `yq` (mikefarah/yq), not the Python wrapper
