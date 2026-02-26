---
status: in-progress
created: 2026-02-26
priority: medium
tags:
- sdk
- cli
- developer
- skills
depends_on:
- 010-claw-runtime-interface
parent: 009-orchestration-platform
created_at: 2026-02-26T02:08:29.576054643Z
updated_at: 2026-02-26T03:07:30.691255720Z
transitions:
- status: in-progress
  at: 2026-02-26T03:07:30.691255720Z
---
# Cross-Claw Developer SDK & CLI

## Overview

A unified SDK and CLI that enables developers to build skills/plugins that work across multiple claw runtimes. The **CLI is Rust** (clap, ships as part of the `clawden` binary). The **Skill SDK is TypeScript** (`@clawden/sdk`) since most skill authors work in TS/JS.

## Design

### CLI (`clawden` — Rust/clap, same binary as server)
```bash
clawden init                    # Initialize ClawDen project
clawden server start            # Start the orchestration server
clawden agent list              # List registered agents
clawden agent start <name>      # Start an agent
clawden agent stop <name>       # Stop an agent
clawden agent health            # Fleet health summary
clawden fleet status            # Fleet overview
clawden task send <agent> <msg> # Send task to agent
clawden skill create <name>     # Scaffold a new skill (generates TS template)
clawden skill test <name>       # Test skill across runtimes
clawden skill publish <name>    # Publish to marketplace
clawden config set <key> <val>  # Set config value
clawden config diff             # Show config drift
```

### Skill SDK
```typescript
import { defineSkill } from '@clawden/sdk';

export default defineSkill({
  name: 'web-scraper',
  version: '1.0.0',
  runtimes: ['openclaw', 'zeroclaw', 'picoclaw'], // compatible runtimes
  tools: ['browser_open', 'http_request'],         // required tools
  
  async execute(context: SkillContext) {
    // Runtime-agnostic skill logic
  },
  
  // Per-runtime adaptations
  adapters: {
    openclaw: { /* OpenClaw-specific config */ },
    zeroclaw: { /* ZeroClaw-specific config */ },
  }
});
```

### Skill Marketplace
- Package registry (npm-style) for cross-claw skills
- Compatibility matrix showing which runtimes are supported
- Version management and dependency resolution

## Plan

- [x] Build CLI subcommands in `clawden-cli` crate (clap derive)
- [x] Implement agent management commands (list, start, stop, health)
- [x] Implement fleet status commands
- [x] Define TypeScript Skill SDK with `defineSkill` API (`sdk/` directory)
- [x] Build skill scaffolding (`clawden skill create` → generates TS template)
- [ ] Create cross-runtime skill test harness
- [ ] Design marketplace registry protocol

## Test

- [x] CLI commands execute correctly against running ClawDen
- [ ] Skill SDK produces valid skill packages
- [ ] Test harness runs skills against multiple runtimes
- [ ] Published skills can be installed and executed