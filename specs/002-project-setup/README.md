---
status: planned
created: '2026-02-03'
tags:
  - setup
  - infrastructure
priority: high
parent: 001-clawlab-mvp
created_at: '2026-02-03T07:37:33.141032008+00:00'
---

# Project Setup & Dependencies

> **Status**: planned · **Priority**: high · **Created**: 2026-02-03

## Overview

Initialize the ClawLab project with proper TypeScript/Node.js configuration and all required dependencies for browser automation, AI vision, and video rendering.

## Design

### Directory Structure
```
clawlab/
├── src/
│   ├── agent/           # Vision agent logic
│   ├── recorder/        # Frame & metadata capture
│   ├── renderer/        # Remotion video composition
│   └── cli/             # Command-line interface
├── package.json
├── tsconfig.json
└── .env.example
```

### Core Dependencies
- **playwright**: Browser automation
- **@remotion/cli, @remotion/renderer**: Video rendering
- **openai** / **@anthropic-ai/sdk**: Vision AI providers
- **commander**: CLI framework
- **zod**: Schema validation

### Dev Dependencies
- **typescript**: Type safety
- **tsx**: TypeScript execution
- **vitest**: Testing framework
- **eslint** + **prettier**: Code quality

## Plan

- [ ] Create `package.json` with all dependencies
- [ ] Set up `tsconfig.json` with strict mode
- [ ] Create directory structure (`src/agent`, `src/recorder`, `src/renderer`, `src/cli`)
- [ ] Add `.env.example` for API keys
- [ ] Configure ESLint and Prettier
- [ ] Add npm scripts (build, dev, test, lint)

## Test

- [ ] `npm install` completes without errors
- [ ] TypeScript compilation succeeds
- [ ] ESLint passes with no errors
- [ ] All directory paths resolve correctly

## Open Questions

1. **Logging framework**: pino vs winston vs console? Need structured logging for debugging agent behavior.
2. **Config file format**: `.clawlabrc`, `clawlab.config.js`, or just `.env`? Should support JSON schema for IDE autocomplete.
3. **Node.js version**: Minimum supported version? (Playwright requires Node 18+)
4. **Monorepo vs single package**: Keep all modules in one package or split for independent versioning?
5. **CI/CD pipeline**: GitHub Actions? What's the build/test/publish flow?

## Notes

This is the foundational spec - all other modules depend on this being complete first.
