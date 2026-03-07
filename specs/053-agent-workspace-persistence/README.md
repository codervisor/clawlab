---
status: in-progress
created: 2026-03-06
priority: high
tags:
- core
- memory
- persistence
- workspace
- git
depends_on:
- 054-agent-fleet-execution-layer
created_at: 2026-03-06T07:10:43.312124552Z
updated_at: 2026-03-07T02:33:38.764062Z
transitions:
- status: in-progress
  at: 2026-03-07T02:33:38.764062Z
---
# Agent Workspace Persistence — Git-Backed Memory Sync & Recovery

> **Status**: in-progress · **Priority**: high · **Created**: 2026-03-06

## Overview

AI agents running on ephemeral infrastructure (GitHub Codespaces, cloud VMs, containers) lose their entire workspace — memory files, identity, user context, tools config — when the host is destroyed. This makes agents amnesiac across infrastructure cycles.

ClawDen should treat agent memory as a first-class managed resource: automatically persisting each agent's workspace to a durable backend and restoring it on fresh deployments. The agent shouldn't need to solve its own persistence — ClawDen handles it.

### Why Now

- Agents accumulate valuable context over time: user preferences, project knowledge, decision history, relationship nuance
- Ephemeral compute (Codespaces, spot instances, autoscaling containers) is the default deployment model
- Manual backup is fragile — one forgotten push and weeks of context are lost
- This is table stakes for any serious agent fleet: agents must survive infrastructure churn

## Design

### Git as the Persistence Backend

Git is the natural choice for agent workspaces:
- **Versioned history** — full audit trail of how memory evolved
- **Conflict resolution** — built-in merge for multi-device scenarios
- **Free hosting** — GitHub/GitLab private repos at zero cost
- **Auth already solved** — tokens, SSH keys, GitHub Apps
- **Diffable** — memory is markdown/JSON, perfect for git

### Architecture

```
┌───────────────────────────────────────────────────┐
│                   ClawDen CLI/Server               │
│                                                    │
│  ┌──────────────────────────────────────────────┐  │
│  │         Workspace Persistence Manager         │  │
│  │                                               │  │
│  │  Per-agent config (in clawden.yaml):          │  │
│  │    workspace:                                 │  │
│  │      repo: codervisor/agent-memory            │  │
│  │      path: agents/{agent-name}/               │  │
│  │      sync_interval: 30m                       │  │
│  │      auto_restore: true                       │  │
│  │                                               │  │
│  │  Operations:                                  │  │
│  │    clawden workspace sync [agent]             │  │
│  │    clawden workspace restore [agent]          │  │
│  │    clawden workspace status [agent]           │  │
│  │    clawden workspace history [agent]          │  │
│  └──────┬──────────────────────────┬─────────────┘  │
│         │                          │                │
│    ┌────▼────┐              ┌──────▼──────┐         │
│    │  Sync   │              │  Restore    │         │
│    │  Engine │              │  Engine     │         │
│    │         │              │             │         │
│    │ Watch → │              │ Clone/pull  │         │
│    │ Commit →│              │ → workspace │         │
│    │ Push    │              │   init      │         │
│    └─────────┘              └─────────────┘         │
└───────────────────────────────────────────────────┘
```

### Docker Bootstrap Integration

The Docker entrypoint is the primary consumer of `clawden workspace restore`. Instead of raw git operations in shell, the entrypoint delegates to the CLI:

```
entrypoint.sh
  │
  ├─ if CLAWDEN_MEMORY_REPO is set:
  │    exec clawden workspace restore \
  │      --repo "$CLAWDEN_MEMORY_REPO" \
  │      --token "$CLAWDEN_MEMORY_TOKEN" \
  │      --target "$CLAWDEN_MEMORY_PATH" \
  │      --branch "$CLAWDEN_MEMORY_BRANCH"
  │
  └─ then launch runtime as usual
```

**Environment variables** (Docker-specific convenience, all map to CLI flags):

| Env Var                 | CLI Flag   | Default       | Description                               |
| ----------------------- | ---------- | ------------- | ----------------------------------------- |
| `CLAWDEN_MEMORY_REPO`   | `--repo`   | —             | Git repo URL or `owner/repo` shorthand    |
| `CLAWDEN_MEMORY_TOKEN`  | `--token`  | —             | Auth token for private repos (GitHub PAT) |
| `CLAWDEN_MEMORY_PATH`   | `--target` | workspace dir | Clone destination                         |
| `CLAWDEN_MEMORY_BRANCH` | `--branch` | `main`        | Git branch                                |

**docker-compose.yml** passes these through so users only need a `.env` file:

```yaml
services:
  openclaw:
    environment:
      - CLAWDEN_MEMORY_REPO=${CLAWDEN_MEMORY_REPO:-}
      - CLAWDEN_MEMORY_TOKEN=${CLAWDEN_MEMORY_TOKEN:-}
```

**Key design decisions:**
- **Best-effort**: restore failure logs a warning but does not block runtime start
- **Token scrubbing**: credentials never appear in logs (grep -v on git output, `--token` treated as secret in CLI)
- **Idempotent**: if workspace already has `.git`, pull instead of clone
- **Shorthand support**: `codervisor/agent-memory` expands to `https://github.com/codervisor/agent-memory.git`

### Sync Engine

Runs as a background task within `clawden up` or triggered by the agent's heartbeat:

1. **Change detection**: `git status` on agent workspace directory
2. **Smart commit**: Only commit if meaningful changes exist (skip if only timestamps changed)
3. **Push**: Push to configured remote with retry + exponential backoff
4. **Conflict handling**: If remote has diverged (e.g., agent ran on two hosts), pull with rebase. Memory files are append-friendly markdown, so conflicts are rare and resolvable.

Sync interval is configurable per-agent. Default: 30 minutes. Critical agents (leader/coordinator) can sync more frequently.

### Restore Engine

Triggered on `clawden up`, `clawden workspace restore`, or Docker entrypoint when workspace is empty/missing:

1. Check if `workspace.repo` is configured (clawden.yaml) or `CLAWDEN_MEMORY_REPO` is set (Docker env)
2. Build authenticated URL — insert token into HTTPS URL, support `owner/repo` shorthand
3. Clone (or fast-forward pull if `.git` exists) into the agent's workspace path
4. Verify workspace integrity (key files exist)
5. Signal agent ready — the runtime reads restored files on startup

### Multi-Agent Layout

A single repo can host multiple agents using path prefixes:

```
codervisor/agent-memory/
├── agents/
│   ├── coordinator/
│   │   ├── MEMORY.md
│   │   ├── IDENTITY.md
│   │   ├── USER.md
│   │   └── memory/
│   │       └── 2026-03-06.md
│   ├── coder-1/
│   │   ├── MEMORY.md
│   │   └── memory/
│   └── researcher/
│       ├── MEMORY.md
│       └── memory/
└── shared/              # Optional: shared context across agents
    └── PROJECT.md
```

### Fleet Config Extension

```yaml
# clawden.yaml
agents:
  coordinator:
    runtime: openclaw
    workspace:
      repo: codervisor/agent-memory
      path: agents/coordinator
      sync_interval: 15m
      auto_restore: true

  coder-1:
    runtime: zeroclaw
    workspace:
      repo: codervisor/agent-memory
      path: agents/coder-1
      sync_interval: 1h
```

### Security

- Workspace repos MUST be private — they contain personal context, preferences, and potentially sensitive project knowledge
- ClawDen validates repo visibility before first sync and warns if public
- Git auth reuses existing `GITHUB_TOKEN` or SSH key config from ClawDen's credential store
- `.gitignore` excludes runtime internals (`.openclaw/`, credentials, temp files)
- Tokens are never logged — all git output is scrubbed before display

## Plan

- [x] **Phase 1: Restore CLI** — Implement `clawden workspace restore` as a Rust command in `clawden-cli`. Handles clone/pull, token auth, shorthand expansion, token scrubbing. This is the foundation everything else builds on.
- [x] **Phase 2: Docker Bootstrap** — Update `docker/entrypoint.sh` to delegate to `clawden workspace restore` instead of raw git. Add `CLAWDEN_MEMORY_*` env vars to `docker-compose.yml`. Remove shell-level git logic from entrypoint.
- [x] **Phase 3: Sync Engine** — Implement `clawden workspace sync` for push-back. Smart commit (skip timestamp-only changes), token injection for push, credential cleanup after push.
- [x] **Phase 5: Status** — Implement `clawden workspace status` showing remote, branch, last commit, dirty file count.
- [x] **Phase 4: Auto-Sync** — Background sync task that runs on a configurable interval during `clawden up`. Integrate with process supervisor.
- [x] **Phase 6: Config Integration** — Add `workspace:` section to `clawden.yaml` schema. Read repo/token/branch from config when CLI flags are omitted.
- [x] **Phase 7: Multi-Agent Layout** — Support path-prefixed multi-agent repos. Shared context directory for cross-agent knowledge.

## Test

- [x] `clawden workspace restore --repo owner/repo --token TOKEN` clones into target dir
- [x] `clawden workspace restore` on existing `.git` dir does fast-forward pull instead of clone
- [x] Token is never visible in stdout/stderr during restore or sync (9 unit tests: URL building, token injection/stripping, credential scrubbing)
- [x] Docker entrypoint with `CLAWDEN_MEMORY_REPO` set calls `clawden workspace restore` and starts runtime
- [x] Docker entrypoint without `CLAWDEN_MEMORY_REPO` skips restore and starts runtime normally
- [x] Restore failure logs warning but runtime still starts (best-effort)
- [x] Sync engine commits and pushes workspace changes to a test repo
- [x] Two agents in same repo with different paths don't interfere with each other
- [x] Conflict scenario: modify workspace on two hosts, verify rebase resolves cleanly
- [x] Public repo detection: `clawden workspace restore` warns if repo is not private

## Implementation Details

### Files Changed

| File                                           | Change                                                                                                                                            |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/clawden-cli/src/commands/workspace.rs` | New — restore, sync, status commands with credential scrubbing                                                                                    |
| `crates/clawden-cli/src/cli.rs`                | Added `Workspace` variant to `Commands` enum + `WorkspaceCommand` subcommand enum                                                                 |
| `crates/clawden-cli/src/commands/mod.rs`       | Added `mod workspace` + `pub use workspace::exec_workspace`                                                                                       |
| `crates/clawden-cli/src/main.rs`               | Added dispatch for `Commands::Workspace`                                                                                                          |
| `crates/clawden-cli/Cargo.toml`                | Added `regex-lite` dependency for credential scrubbing                                                                                            |
| `docker/entrypoint.sh`                         | Replaced 40-line raw git block with 5-line `clawden workspace restore` delegation                                                                 |
| `docker/docker-compose.yml`                    | Added `CLAWDEN_MEMORY_*` env var passthrough for both services                                                                                    |
| `crates/clawden-config/src/lib.rs`             | Added `WorkspaceYaml` struct, `workspace` field to `ClawDenYaml` and `RuntimeEntryYaml`, duration parser, env var resolution for workspace tokens |
| `crates/clawden-cli/src/commands/up.rs`        | Integrated auto-sync: spawns background sync threads on `clawden up`, joins on shutdown                                                           |
| `crates/clawden-cli/src/commands/run.rs`       | Added `workspace: None` to `empty_clawden_yaml()`                                                                                                 |

### Unit Tests (9 + 11 = 20 passing)

- `build_repo_url_shorthand` — `owner/repo` → full GitHub URL
- `build_repo_url_shorthand_with_token` — shorthand + token injection
- `build_repo_url_full_https` / `build_repo_url_full_https_with_token` — full URL passthrough
- `build_repo_url_rejects_invalid` — single-segment names rejected
- `strip_token_round_trip` / `strip_token_noop_for_clean_url` — token stripping
- `scrub_credentials_removes_token` — regex scrubbing of `x-access-token:*@`
- `inject_token_replaces_existing_creds` — replaces old credentials in URL
- `parse_duration_30m` / `parse_duration_1h` / `parse_duration_2h30m` / `parse_duration_90s` / `parse_duration_1h30m15s` — duration string parsing
- `parse_duration_bare_number_as_minutes` / `parse_duration_empty_defaults` — edge cases
- `workspace_yaml_defaults` / `workspace_yaml_custom_values` — struct accessors
- `workspace_yaml_roundtrip` — full YAML → struct deserialization
- `multi_runtime_workspace_yaml` — per-runtime workspace config in multi-runtime mode

## Notes

### Real-World Validation

This spec was born from a live problem: an OpenClaw agent running in Docker had its memory persisted at `~/.openclaw/workspace` to `github.com/codervisor/agent-memory` (private), with frequent sync to the remote. When the container is recreated, there's no way to bootstrap that memory back. The entrypoint needs to restore it before the runtime starts — and that logic belongs in the CLI, not as raw shell in the entrypoint.

### Alternatives Considered

- **S3/GCS blob storage**: Loses versioning, diffability, and free hosting. Git is better for text-heavy workspaces.
- **SQLite in fleet orchestration**: That's for fleet state (agents, tasks, routing). Workspace memory is conceptually different — it's the agent's own cognitive state, not ClawDen's operational state.
- **Runtime-native solutions**: Some runtimes may have their own persistence (e.g., OpenClaw's memory system). ClawDen's approach is runtime-agnostic and works as a safety net regardless.
- **Raw shell in entrypoint**: Works as a quick fix but duplicates logic, can't be tested, and doesn't integrate with `clawden.yaml` config or the CLI workflow. The CLI should own this.

### Open Questions

- Should ClawDen support non-Git backends (S3, local rsync) as plugins? Start with Git only, add later if needed.
- Should there be a `clawden workspace diff` that shows what changed since last sync? Useful for debugging agent memory drift.
- Memory pruning: should ClawDen help agents trim old daily logs? Or leave that to the agent's own housekeeping?
