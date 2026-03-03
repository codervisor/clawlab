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

The built-in tool catalog is **fixed at two tiers**. Extended capabilities are handled by image variants + agent skills (see below).

| Tier         | Install Strategy                                     | Image Impact            | Description                             |
| ------------ | ---------------------------------------------------- | ----------------------- | --------------------------------------- |
| **Core**     | Baked into base image, always present                | Minimal (~50MB total)   | Fundamental utilities every agent needs |
| **Standard** | Pre-installed in default image, activated on request | Moderate (~200MB total) | Common capabilities most agents use     |

Extended capabilities ship as **use-case image variants** (no tool manifests — just pre-installed OS binaries):

| Image Tag   | Includes                                 | ~Size  | Use Case                                                                  |
| ----------- | ---------------------------------------- | ------ | ------------------------------------------------------------------------- |
| `:latest`   | Core + Standard tools                    | ~650MB | General-purpose agent runtime                                             |
| `:browser`  | `:latest` + headless Chromium/Playwright | ~1.1GB | LLM web browsing — search, scrape, fill forms                             |
| `:computer` | `:browser` + Xvfb/VNC/noVNC/fluxbox      | ~1.3GB | Full computer agent — GUI interaction, visual browser, desktop automation |
| `:full`     | `:computer` + compiler toolchain         | ~1.6GB | Advanced users — native compilation, build from source                    |

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

#### Extended Capabilities (image variants only — no tool manifests)

Extended capabilities are provided by **image variants** at the OS level and by **agent skills** at the UX level. They do not have `manifest.toml` / `setup.sh` — the Dockerfile multi-stage build installs the binaries directly.

| Capability | Components                        | ~Size | Image Variant | Agent-Facing UX                 |
| ---------- | --------------------------------- | ----- | ------------- | ------------------------------- |
| Browser    | Chromium 130+, Playwright 1.49+   | 450MB | `:browser`    | `agent-browser` skill           |
| GUI        | Xvfb, x11vnc, noVNC, fluxbox      | 200MB | `:computer`   | Computer-use skills             |
| Compiler   | gcc, g++, make, cmake, pkg-config | 250MB | `:full`       | Direct CLI use (advanced users) |

### Tool Manifest Format

Every tool has a `manifest.toml` alongside its `setup.sh`:

```toml
# /opt/clawden/tools/python/manifest.toml
[tool]
name = "python"
description = "Python 3.12 runtime with pip and venv"
version = "3.12"
tier = "standard"             # core | standard
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

### Extended Capabilities via Image Variants + Skills

Extended capabilities (browser, GUI, compiler) are **not built-in tools** — they have no `manifest.toml` or `setup.sh`. Instead:

- **Image variants** (`:browser`, `:computer`, `:full`) pre-install OS-level binaries at Docker build time
- **Agent skills** (e.g., `agent-browser`) provide the agent-facing UX on top of those binaries
- **Runtime plugins** bridge skills to the LLM

This means the tool catalog is **fixed at Core + Standard tiers**. The entrypoint's activation pipeline, capabilities file, and `CLAWDEN_TOOLS` env var only cover those tiers. Extended binaries are always available in the matching image variant — no activation step needed.

### The `core-utils` Tool

New addition — always-available utilities that are too small to justify separate tools but universally useful:

```bash
# setup.sh
apt-get install -y -qq jq yq tree file zip unzip gzip 2>/dev/null || true
echo "[clawden/tools/core-utils] jq $(jq --version), tree, file, zip ready"
```

These are baked into the base image. `setup.sh` is a validation-only no-op.

### Tool Management CLI

```bash
clawden tools list                     # all Core + Standard tools with tier, size, status
clawden tools list --installed         # only activated tools
clawden tools info python              # detailed info for one tool
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
```

### Skill Integration

Built-in tools and agent skills are complementary layers:

| Layer              | Responsibility                                 | Example                                                              |
| ------------------ | ---------------------------------------------- | -------------------------------------------------------------------- |
| **Built-in tool**  | OS substrate — binaries, servers, env vars     | `python` tool activates Python 3.12 + pip                            |
| **Image variant**  | Pre-installed extended binaries                | `:browser` image ships Chromium + Playwright                         |
| **Agent skill**    | Agent-facing UX — CLI commands the LLM invokes | `agent-browser` provides `open`, `snapshot`, `click`                 |
| **Runtime plugin** | LLM-callable action within a specific runtime  | OpenClaw "web_search" plugin calls `agent-browser` or Playwright API |

**How they connect:**

1. **`SkillDefinition.tools`**: The SDK's `SkillDefinition` (in `@clawden/sdk`) declares which built-in tools a skill needs via its `tools: string[]` field. A skill declaring `tools: ["python"]` communicates that the `python` built-in tool must be activated.

2. **`tools.json` validation**: At `install_skill()` time, the adapter checks `/run/clawden/tools.json` (the capabilities file written by the entrypoint) to verify all declared tool dependencies are activated. If a skill requires `python` but the container was started without it, installation fails with a clear message:
   ```
   Skill "data-analysis" requires tool "python" which is not activated.
   Restart with: clawden up --with python
   ```

3. **Marketplace filtering**: `MarketplaceSearchQuery.tool` already supports filtering skills by required tool, letting users discover skills compatible with their activated tool set.

4. **Lazy install trigger**: When a skill declares a tool dependency and lazy install is enabled, `install_skill()` can automatically trigger tool activation instead of failing.

### Image Layering

Docker layers are ordered so each use-case variant extends the previous one:

```
Layer 1: debian:bookworm-slim + Node.js 22           ~200MB  ─┐
Layer 2: Core tools (git, http, core-utils)            ~50MB   │ :latest
Layer 3: Standard tools (python, code-tools, etc.)    ~170MB   │
Layer 4: Claw runtimes (zeroclaw, openclaw, etc.)      ~100MB ─┘
Layer 5: Chromium + Playwright                         ~450MB ─── :browser
Layer 6: Xvfb + x11vnc + noVNC + fluxbox               ~200MB ─── :computer
Layer 7: gcc, g++, make, cmake                         ~250MB ─── :full
```

Each variant is an additive layer on the previous. Layers 2–3 change infrequently, so most rebuilds only touch Layer 4. The `:browser` → `:computer` → `:full` chain shares all lower layers, so pulling `:computer` when you already have `:browser` only downloads ~200MB.

## Plan

### Phase 1: Foundation
- [x] Add `manifest.toml` to existing `git` and `http` tools
- [x] Create `core-utils` tool (jq, yq, tree, file, zip)
- [x] Update `entrypoint.sh` to read manifests, resolve dependencies, write capabilities file
- [x] Add `core-utils` to Dockerfile base image layer
- [x] Add `CLAWDEN_TOOLS` summary env var export

### Phase 2: Standard Tools
- [x] Create `python` tool (Python 3.12, pip, venv)
- [x] Create `code-tools` tool (ripgrep, fd-find, bat)
- [x] Create `database` tool (SQLite 3)
- [x] Create `network` tool (netcat, socat, dnsutils)
- [x] Create `sandbox` tool (bubblewrap + `clawden-sandbox` wrapper)
- [x] Add standard tools to Dockerfile as separate layer
- [x] Implement `clawden tools list` and `clawden tools info` CLI commands

### Phase 3: Use-Case Image Variants
- [x] Build `:browser` image variant (`:latest` + Chromium/Playwright layer)
- [x] Build `:computer` image variant (`:browser` + Xvfb/VNC/noVNC layer)
- [x] Build `:full` image variant (`:computer` + compiler toolchain layer)
- [ ] Add skill→tool dependency validation: check `tools.json` against `SkillDefinition.tools` at `install_skill()` time

Extended capabilities (browser automation, GUI interaction, compilation) are provided by the image variants at the OS level and by **agent skills** at the UX level. No additional built-in tool manifests or `setup.sh` scripts are needed beyond Core and Standard tiers — skills like `agent-browser` handle the agent-facing interface directly.

## Test

- [ ] `clawden run zeroclaw --with git,python` — Python 3 available inside runtime
- [ ] Skill with `tools: ["python"]` fails `install_skill()` when python tool is not activated
- [ ] Skill with `tools: ["python"]` succeeds when python tool is activated
- [ ] `tools.json` capabilities file written with correct versions after activation
- [ ] `sandbox` isolates execution — sandboxed process cannot access network, cannot read files outside workspace
- [ ] `sandbox` enforces memory/CPU/time limits; attempts to exceed limits are terminated and audited
- [ ] Multi-tenant fail-closed behavior: if sandbox primitives are unavailable, untrusted execution is denied
- [ ] `clawden tools list` shows all tools with tier, size, and status
- [ ] Image sizes: `:latest` < 700MB, `:browser` < 1.2GB, `:computer` < 1.4GB, `:full` < 1.7GB

## Notes

- `core-utils` replaces ad-hoc installs scattered across runtime configs — single source for common CLI tools
- `sandbox` is the highest-priority standard tool — without it, agents running arbitrary code are a security liability
- Extended capabilities (browser, GUI, compiler) are handled by image variants + agent skills — no extended-tier tool manifests needed
- `SkillDefinition.tools` in `@clawden/sdk` is the contract between skills and built-in tools — skills declare, tools.json validates
- Node.js is already in the base image for OpenClaw/NanoClaw
- Image variants form a strict chain: `:latest` ⊂ `:browser` ⊂ `:computer` ⊂ `:full` — each adds one layer
- Direct-install mode (spec 022) uses the same manifests and `setup.sh` scripts for Core/Standard tools
- `yq` refers to the Go-based `yq` (mikefarah/yq), not the Python wrapper

- Progress update (2026-03-03): Docker multi-target stages for `browser`, `computer`, and `full` are implemented in `docker/Dockerfile`, so the image-variant build milestones are complete even though the corresponding tool manifests/setup scripts remain pending.
- Decision (2026-03-03): `browser` tool scoped to substrate-only after confirming `agent-browser` skill covers agent-facing UX. `compiler` tool deferred — no skill or runtime currently requires it beyond what the `:full` image variant already provides. Added skill→tool dependency validation to Phase 3.
- Decision (2026-03-03): Removed Phase 4 (ecosystem/scaling) and all extended-tier tool manifests from Phase 3. Agent skills provide the extensibility layer — the built-in tool catalog is fixed at Core + Standard tiers. Image variants supply OS-level binaries for extended use cases; skills handle the agent-facing interface.