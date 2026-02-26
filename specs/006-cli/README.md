---
status: archived
created: 2026-02-03
priority: medium
tags:
- core
- cli
- interface
depends_on:
- 003-vision-agent
created_at: 2026-02-03T07:37:55.355319483Z
updated_at: 2026-02-26T02:06:31.993704939Z
transitions:
- status: archived
  at: 2026-02-26T02:06:31.993704939Z
parent: 001-clawden-mvp
---

# Command Line Interface

> **Status**: planned · **Priority**: medium · **Created**: 2026-02-03

## Overview

A simple command-line interface to trigger the ClawDen agent, configure settings, and manage demo generation workflows.

## Design

### Core Files
- `src/cli/index.ts` - CLI entry point
- `src/cli/commands/run.ts` - Run demo generation
- `src/cli/commands/render.ts` - Render recorded session
- `src/cli/commands/config.ts` - Manage configuration

### CLI Commands
```bash
# Run agent with a goal
clawden run --url "https://app.example.com" --goal "Show how to create a team"

# Render existing session to video
clawden render --session ./sessions/abc123 --output demo.mp4

# Configure settings
clawden config set llm.provider claude
clawden config set output.format mp4
```

### Command Options
```
run:
  --url, -u          Target URL to automate
  --goal, -g         High-level goal description
  --provider, -p     LLM provider (claude|gpt|gemini)
  --headless         Run in headless mode (default: false)
  --output, -o       Output path for video
  --max-steps        Maximum agent steps (default: 50)

render:
  --session, -s      Path to recorded session
  --output, -o       Output video path
  --format, -f       Output format (mp4|webm|gif)
  --fps              Frame rate (default: 60)
```

## Plan

- [ ] Set up Commander.js CLI framework
- [ ] Implement `run` command - connect to agent and recorder
- [ ] Implement `render` command - invoke renderer for session
- [ ] Implement `config` command - read/write settings file
- [ ] Add progress indicators and colored output
- [ ] Add `--help` documentation for all commands

## Test

- [ ] `clawden --help` shows all commands
- [ ] `clawden run` triggers agent with correct options
- [ ] `clawden render` produces video from session
- [ ] Invalid options show helpful error messages
- [ ] Exit codes are correct (0 for success, 1 for error)

## Open Questions

1. **Programmatic API**: Should ClawAgent be importable as a library, not just CLI?
2. **Config file location**: Where to store config? `~/.clawden/config.json`? Project-local?
3. **API key management**: Secure storage for LLM API keys? Keychain integration?
4. **Interactive mode**: REPL for step-by-step debugging?
5. **Watch mode**: Automatically re-run on goal file changes?
6. **Output verbosity**: Debug vs normal vs quiet modes?
7. **Dry run**: Preview actions without executing?

## Notes

**Binary Distribution**: Consider using `pkg` or `esbuild` to create standalone binaries for distribution.
