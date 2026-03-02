---
status: in-progress
created: 2026-03-02
priority: medium
tags:
- cli
- install
- ux
- docker-compose-parity
created_at: 2026-03-02T08:09:01.949191519Z
updated_at: 2026-03-02T09:07:57.464469438Z
transitions:
- status: in-progress
  at: 2026-03-02T09:04:49.597723610Z
---

# Runtime Pull & Update — Pre-fetch and Update Management

## Overview

Docker Compose has `docker compose pull` to pre-download images without starting services. ClawDen currently has `clawden install` which downloads + installs runtimes, and `clawden up` / `clawden run` which auto-install missing runtimes. **There is no way to update an already-installed runtime, check for newer versions, or pin runtime versions in `clawden.yaml`.**

This spec extends `clawden install` with upgrade and version management capabilities, and adds version pinning to the project config.

## Decision: Why Not `pull`?

We evaluated three options and chose **Option B: extend `clawden install`**.

Docker Compose `pull` exists because of the Docker image layer model — pulling and running are fundamentally different operations. In ClawDen, installing IS pulling — there's no separate "image cache" vs "running container." The runtime binary IS the artifact. A `pull` command would create a confusing synonym for `install` that doesn't reflect ClawDen's architecture.

| Concern | Docker Compose | ClawDen |
|---|---|---|
| Artifact type | OCI image layers | Binary, npm package, git repo |
| Mutable tags | Yes (`:latest` changes) | Partial (`latest` resolves via GitHub API) |
| Pre-download | `pull` | `install` already does this |
| Auto-fetch on start | Only if image missing | Yes, via `ensure_installed_runtime` |
| Update installed | `pull` re-downloads | **Gap — this spec fills it** |
| Version pinning | Image digest / tag in compose.yaml | `runtime@version` CLI syntax exists, **not in clawden.yaml yet** |

**Key design principles:**

1. **`install` already does the work** — download → extract → symlink is identical for fresh install and update. A flag is simpler than a new command.
2. **Avoids "install vs pull" confusion** — a single command with clear flags is easier to learn.
3. **Precedent** — `pip install --upgrade`, `cargo install --force`, `npm install` (which already updates).
4. **`clawden up` auto-installs but must not auto-upgrade** — upgrades change behavior and should be a deliberate user action.

## Design

### 1. CLI Changes

Add `--upgrade` and `--outdated` flags to the existing `Install` command:

```rust
Commands::Install {
    runtime: Option<String>,
    #[arg(long)]
    all: bool,
    #[arg(long)]
    list: bool,
    #[arg(long, short = 'U')]    // NEW
    upgrade: bool,
    #[arg(long)]                  // NEW
    outdated: bool,
}
```

Usage:

```
clawden install zeroclaw              # Install if missing (existing behavior)
clawden install zeroclaw --upgrade    # Re-install to latest even if present
clawden install --upgrade             # Upgrade all installed runtimes
clawden install --outdated            # Check for available updates (no download)
```

### 2. `--outdated` Behavior

Query upstream sources and compare with locally installed versions:

1. List all installed runtimes via `RuntimeInstaller::list_installed()`
2. For each, query the upstream source for the latest available version:
   - ZeroClaw / PicoClaw: GitHub Releases API (`/repos/{owner}/{repo}/releases/latest`)
   - OpenClaw: npm registry (`https://registry.npmjs.org/openclaw/latest`)
   - NanoClaw: `git ls-remote` on the canonical repo
3. Compare installed version against latest
4. Print a table:

```
Runtime     Installed   Latest    Status
zeroclaw    0.1.7       0.2.1     Update available
picoclaw    0.1.3       0.1.3     Up to date
openclaw    1.0.2       1.1.0     Update available
nanoclaw    main        main      Up to date (git)
```

**Exit codes:** `0` = all up to date, `1` = updates available. This enables CI usage: `clawden install --outdated || echo "Updates available"`.

#### RuntimeInstaller API addition

```rust
pub struct VersionCheck {
    pub runtime: String,
    pub installed: String,
    pub latest: String,
    pub update_available: bool,
}

impl RuntimeInstaller {
    /// Query upstream sources for latest versions and compare with installed.
    pub fn check_for_updates(&self) -> Result<Vec<VersionCheck>> { ... }

    /// Query upstream for a single runtime's latest version.
    pub fn query_latest_version(&self, runtime: &str) -> Result<String> { ... }
}
```

### 3. `--upgrade` Behavior

1. If a runtime is specified: re-install to latest (or to version from clawden.yaml if pinned)
2. If no runtime and `--all` not specified: upgrade all installed runtimes
3. Skip runtimes that are already at the latest version (print "already up to date")
4. Respect version constraints from clawden.yaml if present
5. Audit log: record `runtime.upgrade` events (distinct from `runtime.install`)

#### Upgrade flow

```
clawden install zeroclaw --upgrade
  → query_latest_version("zeroclaw") → "0.2.1"
  → compare with installed "0.1.7"
  → "0.2.1" > "0.1.7" → proceed
  → install_runtime("zeroclaw", Some("0.2.1"))
  → symlink current → 0.2.1
  → audit: runtime.upgrade zeroclaw 0.1.7→0.2.1
  → print "Upgraded zeroclaw 0.1.7 → 0.2.1"
```

### 4. Version Pinning in clawden.yaml

Add an optional `version` field to `RuntimeEntryYaml`:

```rust
pub struct RuntimeEntryYaml {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,          // NEW — version pin
    #[serde(default)]
    pub channels: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub config: HashMap<String, Value>,
}
```

#### Version constraint syntax

| Syntax | Meaning | Example |
|---|---|---|
| `"0.2.1"` | Exact pin | Only install 0.2.1 |
| `"0.2.x"` or `"0.2.*"` | Minor pin | Upgrade within 0.2.x (e.g. 0.2.1 → 0.2.5, but not 0.3.0) |
| `">=0.2.0"` | Minimum floor | Any version ≥ 0.2.0 |
| `"latest"` or omitted | No constraint | Always upgrade to latest (default) |

#### Multi-runtime example

```yaml
runtimes:
  - name: zeroclaw
    version: "0.2.x"           # Upgrade within 0.2 range
    channels: [support-tg]
    tools: [git, http]
    provider: openai

  - name: picoclaw
    version: "0.1.3"           # Pin exact version
    channels: [creative-tg]
    tools: [git, python]
    provider: anthropic

  - name: nanoclaw
    channels: [whatsapp]       # No version = latest
    tools: [git]
```

#### Single-runtime shorthand

For the single-runtime shorthand format, add `version` as a top-level field (mirrors how `runtime`, `provider`, `model` etc. already work):

```yaml
runtime: zeroclaw
version: "0.2.x"
provider: openai
model: gpt-4o-mini
channels:
  telegram:
    token: $TELEGRAM_BOT_TOKEN
```

### 5. Version Resolution Priority

When determining what version to install/upgrade to:

1. **CLI explicit version** (`clawden install zeroclaw@0.2.1`) — highest priority
2. **clawden.yaml version pin** (`version: "0.2.x"`) — second priority
3. **Default** — `latest`

When `--upgrade` is used:
- If clawden.yaml pins `version: "0.2.x"` and installed is `0.2.1`, upgrade to latest `0.2.x` tag
- If clawden.yaml pins `version: "0.1.3"` exactly, skip (already at pinned version)
- If no pin, upgrade to latest

### 6. `ensure_installed_runtime` Enhancement

The auto-install helper that runs during `clawden up` / `clawden run` should respect version pins:

```rust
pub fn ensure_installed_runtime(
    installer: &RuntimeInstaller,
    runtime: &str,
    pinned_version: Option<&str>,   // NEW — from clawden.yaml
) -> Result<InstalledRuntime> {
    if let Some(exe) = installer.runtime_executable(runtime) {
        // Already installed — check version compatibility if pinned
        if let Some(pin) = pinned_version {
            let installed = installer.installed_version(runtime)?;
            if !version_satisfies(&installed, pin) {
                println!("Runtime '{runtime}' installed at {installed} but clawden.yaml requires {pin}. Installing...");
                return installer.install_runtime(runtime, Some(pin));
            }
        }
        // Installed and compatible — return as-is (no auto-upgrade)
        return Ok(/* existing */);
    }

    // Not installed — auto-install
    let version = pinned_version.unwrap_or("latest");
    println!("Runtime '{runtime}' not installed. Installing {version}...");
    installer.install_runtime(runtime, Some(version))
}
```

**Critical rule:** auto-install for missing runtimes, version-compatibility check for pinned versions, but **never auto-upgrade** an unpinned runtime just because a newer release exists.

### 7. Semver Comparison

Add a `version_satisfies(installed: &str, constraint: &str) -> bool` utility:

- Parse installed version as semver (strip leading `v`)
- Match constraint against installed:
  - Exact: `"0.2.1"` → `installed == "0.2.1"`
  - Wildcard: `"0.2.x"` → `installed.major == 0 && installed.minor == 2`
  - Range: `">=0.2.0"` → `installed >= "0.2.0"`
  - `"latest"` → always satisfies

Use the `semver` crate for robust parsing and comparison. Handle non-semver versions (like git refs) gracefully.

### 8. Docker Mode Consideration

When ClawDen runs in Docker mode (`ExecutionMode::Docker`), runtime images are Docker images, not local binaries. In this case:
- `--outdated` queries Docker Hub / GHCR for image tag updates
- `--upgrade` runs `docker pull` for the target images
- Version pinning uses image tags: `version: "0.2.1"` → `zeroclaw:0.2.1`

This is a natural extension but not required in the initial implementation — Docker handles image pulling natively. Can be added as a follow-up.

## Plan

- [x] Add `version` field to `RuntimeEntryYaml` in clawden-config
- [x] Add `version` as top-level field in `ClawDenYaml` for single-runtime shorthand
- [x] Add version constraint parsing (`version_satisfies()`) with semver crate
- [x] Add `--upgrade` flag to `Commands::Install` in cli.rs
- [x] Add `--outdated` flag to `Commands::Install` in cli.rs
- [x] Implement `query_latest_version()` in RuntimeInstaller (GitHub API / npm registry)
- [x] Implement `check_for_updates()` in RuntimeInstaller
- [x] Implement `--outdated` output formatting and exit codes in exec_install
- [x] Implement `--upgrade` logic: compare versions, re-install if newer available, respect pins
- [x] Handle `latest` → `latest` case (compare GitHub release tag with installed tag)
- [x] Update `ensure_installed_runtime()` to accept and enforce version pins from config
- [x] Add audit logging for upgrade events (`runtime.upgrade` distinct from `runtime.install`)
- [x] Validate version pins in config validation (`ClawDenYaml::validate()`)

## Test

- [x] `clawden install --outdated` shows correct version table for all installed runtimes
- [x] `clawden install --outdated` exits 0 when all up to date, exits 1 when updates available
- [ ] `clawden install zeroclaw --upgrade` re-installs to latest version
- [ ] `clawden install --upgrade` upgrades all installed runtimes
- [ ] Already up-to-date runtimes are skipped with "already up to date" message
- [x] `--upgrade` without any installed runtimes prints helpful message
- [ ] `clawden up` auto-installs missing runtimes but does NOT auto-upgrade
- [ ] `clawden up` with pinned version installs the pinned version, not latest
- [ ] `clawden up` with incompatible installed version re-installs to match pin
- [ ] `version: "0.2.x"` constraint correctly limits upgrade range
- [ ] `version: "0.1.3"` exact pin prevents any upgrade
- [ ] Omitted version defaults to latest behavior (no constraint)
- [ ] CLI explicit version (`zeroclaw@0.2.1`) overrides clawden.yaml pin
- [x] `version_satisfies()` handles semver, wildcards, ranges, and `latest`
- [x] Non-semver versions (git refs like `main`) are handled gracefully
- [ ] Audit log records `runtime.upgrade` events distinctly from `runtime.install`
- [x] Config validation rejects malformed version constraint strings

## Notes

### Rejected: `clawden pull` as alias

Even as an alias for `install --upgrade`, `pull` creates confusion. Users coming from Docker expect `pull` to only download without installing. But in ClawDen, there's no separation between download and install — they're atomic. An alias that works differently from its Docker counterpart is worse than no alias.

### Rejected: Separate `clawden update` / `clawden outdated` commands

Adding two more top-level commands inflates the CLI surface without adding capability. `install --upgrade` and `install --outdated` keep the lifecycle in one place: `install` handles all "get runtimes onto this machine" concerns.

### Docker mode pull (future)

When running Docker mode, `--upgrade` could shell out to `docker pull`. This is a natural extension but not in initial scope — Docker handles its own image pulling natively, and users can `docker pull` directly.