---
status: archived
created: 2026-02-03
priority: high
tags:
- core
- agent
- ai
depends_on:
- 002-project-setup
created_at: 2026-02-03T07:37:38.319807090Z
updated_at: 2026-02-26T02:06:31.993060955Z
transitions:
- status: archived
  at: 2026-02-26T02:06:31.993060955Z
parent: 001-clawden-mvp
---

# Vision Agent Module

> **Status**: planned · **Priority**: high · **Created**: 2026-02-03

## Overview

Implement the core Vision-Agent that takes a high-level "Goal" (e.g., "Show how to create a team"), captures screenshots via Playwright, sends them to a Vision LLM, and executes the next action (`click`, `type`, `scroll`).

## Design

### Core Files
- `src/agent/ClawAgent.ts` - Main agent orchestration
- `src/agent/BrowserManager.ts` - Playwright browser lifecycle
- `src/agent/ActionExecutor.ts` - Execute click/type/scroll actions
- `src/agent/VisionProvider.ts` - LLM vision API abstraction

### Agent Loop
```
1. Take screenshot of current page state
2. Send screenshot + goal + history to Vision LLM
3. Parse LLM response for next action
4. Execute action with smooth mouse movement
5. Verify action success (wait for selector/state change)
6. Repeat until goal complete or max steps reached
```

### Key Requirements

**State Management**: 
- Verify each action succeeded before proceeding
- Check for specific selectors after clicks
- Detect page navigation/loading states

**Error Handling**:
- Retry logic when LLM "hallucinates" non-existent elements
- Graceful fallback when element not found
- Maximum retry attempts with backoff

### Action Schema (from LLM)
```typescript
type AgentAction = 
  | { type: 'click'; selector: string; description: string }
  | { type: 'type'; selector: string; text: string }
  | { type: 'scroll'; direction: 'up' | 'down'; amount: number }
  | { type: 'complete'; summary: string };
```

## Plan

- [ ] Implement `BrowserManager.ts` - browser launch/close, page management
- [ ] Implement `VisionProvider.ts` - abstract LLM calls (Claude/GPT support)
- [ ] Implement `ActionExecutor.ts` - smooth action execution with Bezier curves
- [ ] Implement `ClawAgent.ts` - main agent loop with state verification
- [ ] Add retry logic for failed actions
- [ ] Add action history tracking for context

## Test

- [ ] Agent can navigate a simple multi-step flow
- [ ] State verification catches failed actions
- [ ] Retry logic handles missing elements gracefully
- [ ] Smooth mouse movements are generated (not teleporting)

## Open Questions

1. **Multi-tab/popup handling**: How to handle new windows, popups, iframes, and shadow DOM?
2. **Authentication flows**: Pre-authenticated sessions or should agent record login? Credential security?
3. **Wait strategies**: Fixed timeout vs dynamic waits? How long before declaring element "not found"?
4. **Action validation**: Screenshot diff, selector check, or network idle? How to verify a click worked?
5. **Viewport size**: Fixed (1920x1080) or configurable? Affects element visibility and demo aesthetics.
6. **Goal completion criteria**: How does LLM signal "done"? Confidence threshold for `complete` action?
7. **Selector strategy**: CSS selectors only, or also XPath and coordinate-based clicking?
8. **LLM rate limits**: Exponential backoff? Fallback to different provider?
9. **Max retries per action**: How many retries before skipping/failing?

## Notes

**Bezier Curve Implementation**: Use cubic Bezier curves for natural mouse movement. Record intermediate points for the Recorder module to capture.
