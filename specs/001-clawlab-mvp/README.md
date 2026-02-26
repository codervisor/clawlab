---
status: archived
created: 2026-02-03
priority: high
tags:
- umbrella
- mvp
created_at: 2026-02-03T07:37:17.371020680Z
updated_at: 2026-02-26T02:06:31.992117036Z
transitions:
- status: archived
  at: 2026-02-26T02:06:31.992117036Z
---

# ClawDen MVP

> **Status**: planned · **Priority**: high · **Created**: 2026-02-03

## Overview

ClawDen is an AI-powered "Browser/Computer Use" engine that automates web interactions to generate high-quality software product demos (videos/images).

**Problem**: Creating polished product demos is time-consuming and requires manual screen recording, editing, and post-production work.

**Solution**: An autonomous AI agent that takes high-level goals (e.g., "Show how to create a team") and produces professional demo videos with smooth animations automatically.

## Design

### Core Tech Stack
- **Language**: TypeScript / Node.js
- **Automation**: Playwright (Headless/Headed)
- **Vision AI**: Mainstream LLMs (Claude, GPT, Gemini, etc.)
- **Video Engine**: Remotion (for rendering smooth UI motion)

### Architecture
```
┌─────────┐     ┌─────────────┐     ┌──────────┐     ┌──────────┐
│   CLI   │────▶│ Vision Agent│────▶│ Recorder │────▶│ Renderer │
└─────────┘     └─────────────┘     └──────────┘     └──────────┘
                      │                   │
                      ▼                   ▼
               ┌──────────────┐    ┌────────────┐
               │ Browser Mgr  │    │  Metadata  │
               │ (Playwright) │    │  + Frames  │
               └──────────────┘    └────────────┘
```

### Key Requirements
1. **Action Smoothing**: Bezier curve mouse movements (not teleporting)
2. **State Management**: Verify action success before proceeding
3. **Error Handling**: Retry logic for AI hallucinations

## Plan

This is an umbrella spec. See child specs for detailed implementation:

- [ ] Project Setup & Dependencies (002)
- [ ] Vision Agent Module (003)
- [ ] Session Recorder Module (004)
- [ ] Video Renderer Module (005)
- [ ] Command Line Interface (006)
- [ ] Module Interfaces & Contracts (007)
- [ ] Vision LLM Prompt Engineering (008)

## Test

- [ ] End-to-end demo generation works for a sample app
- [ ] Generated video has smooth cursor animations
- [ ] Agent can recover from failed actions
- [ ] All modules integrate correctly

## Open Questions (Cross-Cutting)

### Architecture
1. **Session ownership**: Who creates/manages session IDs? Should there be a central `SessionManager`?
2. **Module communication**: Direct imports, event bus, or message queue between modules?
3. **Plugin system**: Should LLM providers and renderers be pluggable at runtime?

### Security
4. **Credential handling**: How to sanitize recordings that capture login flows?
5. **API key storage**: Secure storage for LLM API keys (keychain, encrypted env)?

### Observability  
6. **Logging framework**: pino, winston, or structured console? Debug mode verbosity?
7. **Progress callbacks**: How to surface progress for long-running operations?
8. **Telemetry**: Anonymous usage stats for improvement?

### Edge Cases
9. **Network failures**: How to handle mid-recording connection drops?
10. **Very long sessions**: Memory management for 1000+ frame sessions?
11. **Pages with animations**: Capture strategy for animated content?
12. **Element not found after N retries**: Skip action or fail entire session?

### Distribution
13. **Packaging**: npm package, Docker image, or standalone binary?
14. **Versioning strategy**: Semver for session format compatibility?

## Notes

**Initial Deliverables**:
- `package.json` with all dependencies
- `ClawAgent.ts` - core vision agent logic
- `BrowserManager.ts` - Playwright browser management
