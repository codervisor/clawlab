---
status: archived
created: 2026-02-03
priority: high
tags:
- architecture
- interfaces
- contracts
depends_on:
- 002-project-setup
created_at: 2026-02-03T08:59:22.366459391Z
updated_at: 2026-02-26T02:06:31.993838276Z
transitions:
- status: archived
  at: 2026-02-26T02:06:31.993838276Z
parent: 001-clawden-mvp
---

# Module Interfaces & Contracts

## Overview

Define TypeScript interfaces and contracts that govern communication between ClawDen modules. This ensures loose coupling, testability, and clear boundaries.

## Design

### Core Interfaces File
- `src/types/index.ts` - All shared types and interfaces

### Module Boundary Contracts

#### Agent → Recorder Interface
```typescript
interface IRecorderClient {
  startSession(config: SessionConfig): Promise<RecordingSession>;
  captureFrame(): Promise<Frame>;
  recordCursorPosition(point: CursorPoint): void;
  recordEvent(event: ActionEvent): void;
  endSession(): Promise<SessionSummary>;
}

interface SessionConfig {
  outputDir: string;
  frameRate: number;
  captureFormat: 'png' | 'jpeg' | 'webp';
  viewport: { width: number; height: number };
}
```

#### Recorder → Renderer Interface
```typescript
interface ISessionReader {
  loadSession(sessionPath: string): Promise<RecordingSession>;
  getFrames(): AsyncIterator<Frame>;
  getCursorPath(): CursorPoint[];
  getEvents(): ActionEvent[];
}

interface RenderConfig {
  outputPath: string;
  format: 'mp4' | 'webm' | 'gif';
  fps: number;
  quality: 'low' | 'medium' | 'high';
  cursor?: CursorStyle;
}
```

#### Vision Provider Interface
```typescript
interface IVisionProvider {
  readonly name: string;
  analyze(request: VisionRequest): Promise<VisionResponse>;
}
```

#### Agent Action Types
```typescript
type AgentAction =
  | ClickAction
  | TypeAction
  | ScrollAction
  | WaitAction
  | NavigateAction
  | CompleteAction;

type ElementTarget =
  | { selector: string }
  | { xpath: string }
  | { coordinates: { x: number; y: number } }
  | { text: string; near?: string };
```

### Error Types
```typescript
class ClawDenError extends Error {
  constructor(
    message: string,
    public readonly code: ErrorCode,
    public readonly recoverable: boolean,
    public readonly context?: Record<string, unknown>
  ) { super(message); }
}

type ErrorCode =
  | 'BROWSER_LAUNCH_FAILED'
  | 'ELEMENT_NOT_FOUND'
  | 'ACTION_TIMEOUT'
  | 'LLM_API_ERROR'
  | 'LLM_PARSE_ERROR'
  | 'SESSION_WRITE_ERROR'
  | 'RENDER_FAILED'
  | 'INVALID_CONFIG';
```

## Plan

- [ ] Create `src/types/index.ts` with all interfaces
- [ ] Create `src/types/actions.ts` for action types
- [ ] Create `src/types/events.ts` for event bus
- [ ] Create `src/types/errors.ts` for error taxonomy
- [ ] Add Zod schemas for runtime validation
- [ ] Export all types from barrel file

## Test

- [ ] All interfaces compile without errors
- [ ] Zod schemas validate sample data correctly
- [ ] Error types serialize/deserialize properly
- [ ] Mock implementations satisfy interfaces

## Open Questions

1. **Event bus**: Direct imports vs event-driven architecture? EventEmitter or rxjs?
2. **Dependency injection**: Use a DI container (tsyringe, inversify) or manual wiring?
3. **Async iterator vs callback**: Which pattern for streaming frames to renderer?
4. **Error recovery contracts**: How should modules communicate recoverable vs fatal errors?
5. **Plugin architecture**: Should VisionProvider be pluggable at runtime?

## Notes

**Versioning**: Consider adding a `version` field to `RecordingSession` for forward compatibility with session format changes.