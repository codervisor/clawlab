---
status: planned
created: 2026-03-07
priority: high
tags:
- workspace
- memory
- runtime
- config
- bugfix
parent: 053-agent-workspace-persistence
created_at: 2026-03-07T06:40:05.407138Z
updated_at: 2026-03-07T06:40:13.149417Z
---

# Runtime-Aware Workspace Path Resolution

> **Status**: draft · **Priority**: high · **Created**: 2026-03-07

## Overview

`clawden workspace restore` fails when run from an existing non-empty directory because `resolve_target` defaults to the current working directory. Additionally, restored memory is never wired into the runtime's actual workspace — the runtime still reads from its own default path (e.g. `~/.openclaw/workspace`), meaning the restored memory is ignored.

This spec fixes two problems:
1. **Broken default path**: `resolve_target` clones into CWD, which fails when CWD has existing files
2. **Missing runtime bridge**: no mechanism to point a runtime's workspace at ClawDen's restored memory location

### Reproduction

```sh
cd ~/projects/my-project
clawden workspace restore --repo codervisor/agent-memory
# fatal: destination path '/Users/user/projects/my-project' already exists
#        and is not an empty directory.
# [clawden] Warning: git clone failed, continuing without agent memory
```

## Design

### Runtime Memory Locations (researched via DeepWiki)

| Runtime | State dir | Workspace path | Override env var |
|---------|-----------|---------------|-----------------|
| OpenClaw | `~/.openclaw` | `~/.openclaw/workspace` | `OPENCLAW_HOME` → `$OPENCLAW_HOME/workspace`, or `agents.defaults.workspace` in openclaw.json |
| ZeroClaw | `~/.zeroclaw` | `~/.zeroclaw/workspace` | `ZEROCLAW_WORKSPACE` (direct path) |

Both runtimes store memory as Markdown files in their workspace: `MEMORY.md`, `memory/YYYY-MM-DD.md`, `USER.md`, `IDENTITY.md`, `AGENTS.md`, `SOUL.md`.

### Strategy

ClawDen-managed workspace path + runtime env injection:

1. **`resolve_target` defaults to `.clawden/memory`** under CWD when no runtime context exists
2. **Runtime-aware default**: when a `clawden.yaml` is present with a known runtime, default to the runtime's native workspace path (e.g. `~/.openclaw/workspace`)
3. **Explicit `workspace.path` injection**: when `workspace.path` is set in `clawden.yaml`, inject the corresponding env var into the runtime process so it reads memory from the ClawDen-managed location
4. **Non-empty directory fallback**: when the target exists and is non-empty but has no `.git`, use `git init` + `fetch` + `checkout` instead of `clone`

### Path Resolution Precedence

```
--target flag (highest)
  → workspace.path in clawden.yaml
    → CLAWDEN_MEMORY_PATH env var
      → runtime-native default (if runtime known from config)
        → .clawden/memory under CWD (lowest)
```

### Config Injection

When `workspace.path` is configured, `config_gen.rs` / runtime launch must set:

| Runtime | Env var to set | Value |
|---------|---------------|-------|
| OpenClaw | `OPENCLAW_HOME` | parent of workspace path (since OpenClaw appends `/workspace`) |
| ZeroClaw | `ZEROCLAW_WORKSPACE` | workspace path directly |

For Docker mode, `CLAWDEN_MEMORY_PATH` already controls the restore target; the entrypoint mounts it into the container's runtime workspace via volume or env var.

## Plan

- [ ] **Phase 1: Safe default** — Change `resolve_target` local fallback from CWD to `.clawden/memory`. Add `init_and_fetch` fallback for non-empty non-git directories.
- [ ] **Phase 2: Runtime-aware default** — When `clawden.yaml` specifies a single runtime, `resolve_target` maps to the runtime's native workspace path. Add a `workspace_path` field to `RuntimeDescriptor`.
- [ ] **Phase 3: Workspace env injection** — Extend `state_dir_env_vars` (or add a peer function) in `config_gen.rs` to inject `OPENCLAW_HOME` / `ZEROCLAW_WORKSPACE` when `workspace.path` is configured. Wire into `run` and `up` launch paths.
- [ ] **Phase 4: Tests** — Unit tests for new `resolve_target` behavior, runtime-aware defaults, env injection. Integration test: restore + up verifies runtime sees memory.

## Test

- [ ] `clawden workspace restore` from a non-empty CWD without `--target` clones into `.clawden/memory` instead of failing
- [ ] `clawden workspace restore` in a project with `runtime: openclaw` and no explicit target restores to `~/.openclaw/workspace`
- [ ] Non-empty target directory (no `.git`) succeeds via init+fetch fallback
- [ ] `workspace.path` in `clawden.yaml` causes `OPENCLAW_HOME` to be set for openclaw runtime
- [ ] `workspace.path` in `clawden.yaml` causes `ZEROCLAW_WORKSPACE` to be set for zeroclaw runtime
- [ ] Docker mode with `CLAWDEN_MEMORY_PATH` still works as before (no regression)
- [ ] Token is never visible in stdout/stderr during init+fetch flow

## Notes

- The runtime-native paths were confirmed via DeepWiki research against `openclaw/openclaw` and `zeroclaw-labs/zeroclaw` repositories
- PicoClaw, NanoClaw, IronClaw workspace paths are TBD — add to `RuntimeDescriptor` when researched
- This spec is a child of 053-agent-workspace-persistence (addresses gaps found during real-world usage)
