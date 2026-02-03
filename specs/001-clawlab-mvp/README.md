---
status: planned
created: '2026-02-03'
tags:
  - umbrella
  - mvp
priority: high
created_at: '2026-02-03T07:37:17.371020680+00:00'
---

# ClawLab MVP

> **Status**: planned · **Priority**: high · **Created**: 2026-02-03

## Overview

ClawLab is an AI-powered "Browser/Computer Use" engine that automates web interactions to generate high-quality software product demos (videos/images).

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

## Test

- [ ] End-to-end demo generation works for a sample app
- [ ] Generated video has smooth cursor animations
- [ ] Agent can recover from failed actions
- [ ] All modules integrate correctly

## Notes

**Initial Deliverables**:
- `package.json` with all dependencies
- `ClawAgent.ts` - core vision agent logic
- `BrowserManager.ts` - Playwright browser management
