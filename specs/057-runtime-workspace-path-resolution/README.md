---
status: in-progress
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
updated_at: 2026-03-07T07:07:48.981086Z
transitions:
- status: in-progress
  at: 2026-03-07T07:07:48.981086Z
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

| Runtime  | State dir     | Workspace path          | Override env var                                                                              |
| -------- | ------------- | ----------------------- | --------------------------------------------------------------------------------------------- |
| OpenClaw | `~/.openclaw` | `~/.openclaw/workspace` | `OPENCLAW_HOME` → `$OPENCLAW_HOME/workspace`, or `agents.defaults.workspace` in openclaw.json |
| ZeroClaw | `~/.zeroclaw` | `~/.zeroclaw/workspace` | `ZEROCLAW_WORKSPACE` (direct path)                                                            |

Both runtimes store memory as Markdown files in their workspace: `MEMORY.md`, `memory/YYYY-MM-DD.md`, `USER.md`, `IDENTITY.md`, `AGENTS.md`, `SOUL.md`.

### Strategy

Symlink-based bridging — ClawDen restores to a canonical location and symlinks each runtime's native workspace path to it:

1. **`resolve_target` defaults to `~/.clawden/workspace`** — stable canonical location, consistent with `~/.openclaw/workspace` / `~/.zeroclaw/workspace` naming
2. **Symlink bridge**: after restore, create symlinks from each runtime's native workspace path to `~/.clawden/workspace`:
   - `~/.openclaw/workspace` → `~/.clawden/workspace`
   - `~/.zeroclaw/workspace` → `~/.clawden/workspace`
3. **Non-empty directory fallback**: when the target exists and is non-empty but has no `.git`, use `git init` + `fetch` + `checkout` instead of `clone`

This avoids per-runtime env var injection entirely — the runtime reads from its expected path, which is just a symlink to ClawDen's managed location.

### Symlink Rules

| Scenario                                        | Action                                         |
| ----------------------------------------------- | ---------------------------------------------- |
| Target path doesn't exist                       | Create symlink                                 |
| Target is already a symlink → correct dest      | No-op                                          |
| Target is already a symlink → wrong dest        | Update symlink                                 |
| Target is a real directory (existing workspace) | Back up to `<path>.bak.YYYYMMDD`, then symlink |
| Docker mode                                     | Skip symlinking — volumes handle the mapping   |

### Path Resolution Precedence

```
--target flag (highest)
  → workspace.path in clawden.yaml
    → CLAWDEN_MEMORY_PATH env var
      → ~/.clawden/workspace (lowest)
```

### Why Symlinks Over Env Injection

- **Runtime-agnostic**: no need to know each runtime's workspace env var
- **Transparent**: `ls -la ~/.openclaw/workspace` shows exactly where memory lives
- **Zero config-gen changes**: no modifications to `config_gen.rs` or `state_dir_env_vars`
- **Works with any runtime**: future runtimes work automatically if they follow the `~/.<name>/workspace` convention
- **Reversible**: `rm` the symlink and the runtime goes back to a local workspace

## Plan

- [x] **Phase 1: Safe default** — Change `resolve_target` local fallback from CWD to `~/.clawden/workspace`. Add `init_and_fetch` fallback for non-empty non-git directories.
- [x] **Phase 2: Symlink bridge** — After successful restore, create symlinks from each configured runtime's native workspace path to `~/.clawden/workspace`. Handle existing directories (backup), existing symlinks (update/skip), and Docker mode (skip). Add `workspace_path` field to `RuntimeDescriptor` so ClawDen knows the native path per runtime.
- [x] **Phase 3: Tests** — Unit tests for `resolve_target`, symlink creation/update/backup logic, Docker-mode skip. Integration test: restore creates symlink, runtime sees memory files.

## Test

- [x] `clawden workspace restore` from a non-empty CWD without `--target` clones into `~/.clawden/workspace` instead of failing
- [x] Non-empty target directory (no `.git`) succeeds via init+fetch fallback
- [x] After restore with `runtime: openclaw`, `~/.openclaw/workspace` is a symlink → `~/.clawden/workspace`
- [ ] After restore with `runtime: zeroclaw`, `~/.zeroclaw/workspace` is a symlink → `~/.clawden/workspace`
- [x] Existing real workspace directory is backed up to `*.bak.YYYYMMDD` before symlinking
- [x] Existing correct symlink is left untouched (no-op)
- [x] Existing wrong symlink is updated to point to the correct target
- [x] Docker mode (`$HOME/workspace` exists) skips symlink creation
- [ ] Docker mode with `CLAWDEN_MEMORY_PATH` still works as before (no regression)
- [ ] Token is never visible in stdout/stderr during init+fetch flow

## Notes

- The runtime-native paths were confirmed via DeepWiki research against `openclaw/openclaw` and `zeroclaw-labs/zeroclaw` repositories
- PicoClaw, NanoClaw, IronClaw workspace paths are TBD — add to `RuntimeDescriptor` when researched
- This spec is a child of 053-agent-workspace-persistence (addresses gaps found during real-world usage)