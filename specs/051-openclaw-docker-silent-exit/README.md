---
status: complete
created: 2026-03-06
priority: high
tags:
- docker
- openclaw
- bug
- ux
- container
parent: 017-docker-runtime-images
created_at: 2026-03-06T05:28:14.473366266Z
updated_at: 2026-03-06T05:42:21.141348593Z
completed_at: 2026-03-06T05:42:21.141348593Z
transitions:
- status: in-progress
  at: 2026-03-06T05:37:44.246935761Z
- status: complete
  at: 2026-03-06T05:42:21.141348593Z
---

# OpenClaw Docker Silent Exit — Container Dies After Printing Help, CLI Reports Success

## Overview

`clawden docker run openclaw` launches the container with `RUNTIME=openclaw`. The entrypoint execs into the OpenClaw binary with default args (`gateway` in the shell entrypoint, `gateway --allow-unconfigured` in the Rust descriptor). Without valid channel credentials or provider configuration injected, OpenClaw prints help/usage output and exits. The container appears to stay up briefly only because the Node.js startup is slower — it is not actually running a sustained process.

This means `clawden docker run openclaw` reports "Started openclaw via Docker" but the container dies seconds later. The user sees a false success.

## Problem

The entrypoint's default subcommand for OpenClaw assumes the runtime will enter a long-running server loop. But OpenClaw's `gateway` subcommand exits immediately when it has no configured channels or providers — it prints diagnostic output and returns. This is correct behavior from OpenClaw's perspective (fail fast on misconfiguration), but wrong from the container's perspective (PID 1 exits, container dies).

The same pattern may affect other runtimes whose default subcommand exits on missing config rather than waiting for connections.

### Observed behavior

```
$ clawden docker run openclaw
Started openclaw via Docker

$ docker ps -a --filter name=clawden-openclaw
NAME                              STATUS
clawden-openclaw-openclaw-default Exited (0) 3 seconds ago
```

### Root causes

1. **No pre-flight config check in the entrypoint** — the entrypoint blindly execs into the runtime without verifying that minimum configuration (provider key, at least one channel) is present.
2. **No Docker-side startup verification** — `clawden docker run` prints success as soon as `docker run -d` returns a container ID, without checking whether the container is still alive seconds later.
3. **Entrypoint default args drift** — the shell entrypoint uses `gateway` while the Rust descriptor uses `gateway --allow-unconfigured`. Neither is sufficient: `--allow-unconfigured` may let OpenClaw start without channels, but it still needs a provider key to do anything useful.

## Design

### Option A: Entrypoint pre-flight validation

Before exec-ing into the runtime, the entrypoint checks for minimum required env vars per runtime. If missing, it prints a clear error with the exact env vars needed and exits with a distinct code (e.g., 78 = EX_CONFIG).

```bash
case "$RUNTIME" in
    openclaw)
        if [ -z "${OPENROUTER_API_KEY:-}" ] && [ -z "${OPENAI_API_KEY:-}" ] && [ -z "${ANTHROPIC_API_KEY:-}" ]; then
            echo "[clawden] Error: openclaw requires at least one LLM provider key." >&2
            echo "[clawden] Set one of: OPENROUTER_API_KEY, OPENAI_API_KEY, ANTHROPIC_API_KEY" >&2
            exit 78
        fi
        ;;
esac
```

### Option B: Post-start health gate in adapter

After `docker run -d`, the adapter waits a short grace period (e.g., 3s) and then checks `docker inspect` to see if the container is still running. If not, it pulls the container logs and reports the real failure. (This was partially added in a prior fix but uses an immediate check — a grace period is needed for runtimes that take >0s to fail.)

### Recommendation

Both. Option A catches the problem at the source with an actionable message. Option B catches any runtime failure mode the entrypoint can't predict.

## Plan

- [x] Add per-runtime minimum env var checks to `docker/entrypoint.sh` before the `exec` line
- [x] Align entrypoint default args with Rust `RuntimeDescriptor::default_start_args` (openclaw: `gateway --allow-unconfigured`)
- [x] Add a short grace-period health gate to `start_container()` in the adapter (wait ~3s, then check container state)
- [x] Surface container logs in the CLI error when the container exits during the grace period
- [x] Ensure `clawden docker run openclaw` with valid credentials actually sustains the container

## Test

- [x] `clawden docker run openclaw` without any provider key exits with clear "missing provider key" error, not silent exit
- [x] `clawden docker run openclaw` with valid `OPENROUTER_API_KEY` starts and container remains running
- [x] Entrypoint default args for openclaw match Rust descriptor (`gateway --allow-unconfigured`)
- [x] Container that exits within grace period causes CLI to report failure with logs, not success