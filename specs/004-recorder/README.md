---
status: planned
created: '2026-02-03'
tags:
  - core
  - recorder
  - capture
priority: medium
parent: 001-clawlab-mvp
depends_on:
  - 002-project-setup
created_at: '2026-02-03T07:37:43.628949379+00:00'
---

# Session Recorder Module

> **Status**: planned · **Priority**: medium · **Created**: 2026-02-03

## Overview

Capture high-resolution frames and metadata (element coordinates, event logs, cursor positions) during AI agent sessions. This data feeds into the Renderer module for polished video output.

## Design

### Core Files
- `src/recorder/SessionRecorder.ts` - Main recording orchestration
- `src/recorder/FrameCapture.ts` - Screenshot capture at configurable FPS
- `src/recorder/MetadataCollector.ts` - Event and cursor position logging
- `src/recorder/SessionStore.ts` - Persist session data to disk

### Recording Data Model
```typescript
interface RecordingSession {
  id: string;
  startTime: number;
  frames: Frame[];
  events: ActionEvent[];
  cursorPath: CursorPoint[];
}

interface Frame {
  timestamp: number;
  path: string;  // path to screenshot file
  dimensions: { width: number; height: number };
}

interface CursorPoint {
  timestamp: number;
  x: number;
  y: number;
  isClick: boolean;
}

interface ActionEvent {
  timestamp: number;
  type: 'click' | 'type' | 'scroll' | 'navigate';
  details: Record<string, unknown>;
}
```

### Capture Strategy
- **Screenshot FPS**: Configurable (default 10 FPS for efficiency)
- **Cursor Tracking**: Record Bezier control points from ActionExecutor
- **Event Correlation**: Link events to frame timestamps

## Plan

- [ ] Implement `FrameCapture.ts` - timed screenshot capture using Playwright
- [ ] Implement `MetadataCollector.ts` - collect cursor paths and events
- [ ] Implement `SessionStore.ts` - write frames to disk, metadata to JSON
- [ ] Implement `SessionRecorder.ts` - coordinate capture and storage
- [ ] Add session start/stop/pause controls
- [ ] Optimize frame storage (compression, cleanup)

## Test

- [ ] Frames captured at consistent intervals
- [ ] Cursor path data matches actual mouse movements
- [ ] Events correctly timestamped and correlated with frames
- [ ] Session data persists correctly to disk
- [ ] Large sessions don't run out of memory

## Open Questions

1. **Frame format**: PNG (lossless, large) vs JPEG (lossy, small) vs WebP (balanced)?
2. **Sync mechanism**: How are frame timestamps precisely synchronized with cursor points?
3. **Storage limits**: Max session size? Auto-cleanup policy for old sessions?
4. **Resume capability**: Can recording resume after crash/interruption?
5. **Session versioning**: Version field for forward compatibility with format changes?
6. **Compression**: Compress frames on-the-fly or post-session?
7. **Memory pressure**: What happens during very long sessions (1000+ frames)?

## Notes

**Performance**: Consider writing frames asynchronously to avoid blocking the agent. Use a worker thread or queue for I/O.
