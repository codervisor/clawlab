---
status: planned
created: '2026-02-03'
tags:
  - core
  - renderer
  - remotion
priority: medium
parent: 001-clawlab-mvp
depends_on:
  - 002-project-setup
  - 004-recorder
created_at: '2026-02-03T07:37:49.020299365+00:00'
---

# Video Renderer Module

> **Status**: planned · **Priority**: medium · **Created**: 2026-02-03

## Overview

A Remotion-based video composition that takes captured frames and metadata to produce polished demo videos with smooth cursor interpolation and professional transitions.

## Design

### Core Files
- `src/renderer/DemoComposition.tsx` - Main Remotion composition
- `src/renderer/CursorOverlay.tsx` - Animated cursor component
- `src/renderer/BezierInterpolation.ts` - Smooth cursor path calculation
- `src/renderer/render.ts` - Render pipeline and export

### Remotion Composition Structure
```
DemoComposition
├── FrameSequence (background screenshots)
├── CursorOverlay (animated cursor on top)
└── ClickIndicator (visual feedback for clicks)
```

### Cursor Interpolation
- Input: Discrete cursor points with timestamps
- Output: Smooth Bezier curve animation at 60 FPS
- Use cubic Bezier interpolation between control points
- Ease-in-out for natural movement feel

### Output Options
- **Format**: MP4 (H.264), WebM, GIF
- **Resolution**: Match source frames (e.g., 1920x1080)
- **FPS**: 60 for smooth playback
- **Quality**: Configurable bitrate

## Plan

- [ ] Set up Remotion project structure
- [ ] Implement `BezierInterpolation.ts` - smooth cursor paths from control points
- [ ] Implement `CursorOverlay.tsx` - animated cursor with click indicators
- [ ] Implement `DemoComposition.tsx` - combine frames with cursor overlay
- [ ] Implement `render.ts` - export to video formats
- [ ] Add configuration for output format/quality

## Test

- [ ] Cursor movement appears smooth (no teleporting)
- [ ] Click indicators appear at correct timestamps
- [ ] Output video matches source frame dimensions
- [ ] Rendering completes without errors for sample session
- [ ] Multiple output formats work correctly

## Open Questions

1. **Remotion licensing**: Commercial use implications? Any restrictions for SaaS?
2. **Customization options**: Branding overlays, intro/outro sequences, captions/subtitles?
3. **Render time**: Expected duration for a 2-minute demo? Can we parallelize?
4. **Progress API**: How does CLI get render progress updates for display?
5. **Page with animations**: How to handle pages with existing CSS animations without visual chaos?
6. **Audio track**: Support for voiceover or background music?
7. **Zoom/highlight effects**: Auto-zoom on clicked elements for emphasis?

## Notes

**Remotion Setup**: Requires separate Remotion config. Consider using `@remotion/bundler` for programmatic rendering without the full Remotion studio.
