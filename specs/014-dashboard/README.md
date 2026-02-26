---
status: in-progress
created: 2026-02-26
priority: medium
tags:
- ui
- dashboard
- monitoring
depends_on:
- 011-control-plane
- 012-fleet-orchestration
parent: 009-orchestration-platform
created_at: 2026-02-26T02:08:29.576000081Z
updated_at: 2026-02-26T03:07:30.690129394Z
transitions:
- status: in-progress
  at: 2026-02-26T03:07:30.690129394Z
---
# Unified Web Dashboard

## Overview

A web-based dashboard providing real-time visibility into the claw agent fleet. Operators can monitor health, manage lifecycle, view task routing, and configure agents from a single interface.

## Design

### Views
1. **Fleet Overview** — Map/grid of all registered agents with status indicators
2. **Agent Detail** — Deep dive: health, metrics, logs, config, active tasks
3. **Task Monitor** — Real-time task flow, routing decisions, completion rates
4. **Swarm View** — Visualize multi-agent collaboration topology
5. **Config Editor** — Edit canonical configs with runtime preview
6. **Audit Log** — Searchable history of all lifecycle and task events

### Tech Stack
- **Frontend**: React 19 + Tailwind CSS + shadcn/ui
- **Real-time**: WebSocket for live status updates
- **Charts**: Recharts or Tremor for metrics visualization
- **API**: Consumes ClawDen REST + WebSocket API

### Key Interactions
- Start/stop/restart agents from dashboard
- Drag-and-drop task assignment to agents
- One-click config deployment with diff preview
- Alert configuration and notification management

## Plan

- [ ] Scaffold React app with Tailwind + shadcn/ui
- [x] Build fleet overview grid with status indicators
- [ ] Implement agent detail page with health metrics
- [ ] Add real-time WebSocket status updates
- [ ] Build config editor with diff preview
- [ ] Add task monitor and routing visualization
- [ ] Implement audit log viewer

## Test

- [x] Dashboard loads and displays fleet status
- [ ] Real-time updates reflect agent state changes within 2s
- [ ] Config editor validates before deploying
- [ ] All views are responsive (desktop + tablet)