---
status: planned
created: 2026-02-26
priority: high
tags:
- ui
- dashboard
- ux
- shadcn
- theming
depends_on:
- 014-dashboard
parent: 009-orchestration-platform
created_at: 2026-02-26T08:28:40.087516378Z
updated_at: 2026-02-26T08:28:40.087516378Z
---

# Dashboard UI/UX Enhancement — shadcn/ui, Theming & Polish

## Overview

Upgrade the ClawDen dashboard from raw Tailwind markup to a professional, polished UI built on **shadcn/ui**. Add a light/dark theme toggle, sticky headers, toast notifications, a collapsible sidebar, and other UX refinements so the dashboard feels like a production-grade control plane.

## Motivation

The current dashboard (spec 014) ships a functional single-file React app with hand-rolled Tailwind classes. It lacks:

- A design-system foundation (consistent spacing, colors, radii, typography)
- Dark mode / light mode switching
- Standard UI patterns operators expect (toasts for async feedback, sticky navigation, keyboard shortcuts, collapsible sidebar for more screen real-estate)

This spec addresses all of the above by adopting shadcn/ui as the component layer and layering proper UX patterns on top.

## Design

### 1. shadcn/ui Integration

- Install shadcn/ui CLI and initialize with the project's Tailwind 4 setup
- Adopt the **New York** style variant for a clean, dense look suitable for dashboards
- Use CSS variables for theming so light/dark mode is a single class toggle
- Replace hand-rolled UI elements with shadcn/ui equivalents:
  - `Button`, `Badge`, `Card`, `Table`, `Tabs`, `Dialog`, `DropdownMenu`, `Tooltip`, `Separator`, `ScrollArea`, `Sheet`, `Input`, `Select`, `Command` (for command palette)

### 2. Light / Dark Theme Toggle

- Implement a `ThemeProvider` context using `class`-based dark mode (`<html class="dark">`)
- Persist preference in `localStorage`; default to system preference via `prefers-color-scheme`
- Place a `Sun/Moon` toggle in the top-right of the header bar
- All shadcn/ui CSS variables and custom styles must respect the theme

### 3. Layout & Navigation Overhaul

- **Sticky header**: Top bar with logo/title, breadcrumb, WS-connection indicator, theme toggle, and optional user avatar slot — stays fixed on scroll
- **Collapsible sidebar**: Left sidebar with nav items (Fleet, Tasks, Config, Audit); togglable via hamburger icon or keyboard shortcut (`Cmd/Ctrl + B`); collapsed state shows icons only; state persisted in `localStorage`
- **Content area**: Scrollable main content that consumes remaining viewport; uses `ScrollArea` from shadcn/ui for styled scrollbars
- Responsive: sidebar auto-collapses below `lg` breakpoint; on mobile it becomes a slide-over `Sheet`

### 4. Toast Notifications

- Use shadcn/ui `Sonner` (toast) component for async operation feedback
- Show toasts for: agent start/stop/restart results, config deployment success/failure, WebSocket reconnection events, validation errors
- Toasts stack in bottom-right, auto-dismiss after 5 s, with manual close

### 5. Additional UX Improvements

- **Loading skeletons**: Use shadcn/ui `Skeleton` component while data is fetching
- **Empty states**: Friendly illustration + CTA when no agents are registered
- **Keyboard shortcuts**: `Cmd/Ctrl+K` for command palette, `Cmd/Ctrl+B` for sidebar toggle
- **Confirmation dialogs**: Use `AlertDialog` for destructive actions (stop/restart agent, deploy config)
- **Accessible focus rings & ARIA labels**: Ensure all interactive elements meet WCAG 2.1 AA
- **Smooth transitions**: Sidebar collapse, theme switch, and route changes use CSS transitions (150–200 ms)

### Tech Stack Additions

| Package | Purpose |
|---|---|
| `shadcn/ui` (via CLI) | Component primitives |
| `tailwind-merge` | Merge Tailwind classes safely |
| `class-variance-authority` | Component variant management |
| `clsx` | Conditional class joining |
| `lucide-react` | Icon library (shadcn default) |
| `sonner` | Toast notifications |

### File Structure (target)

```
dashboard/src/
├── components/
│   ├── ui/              # shadcn/ui generated components
│   ├── layout/
│   │   ├── Header.tsx
│   │   ├── Sidebar.tsx
│   │   └── Layout.tsx
│   ├── fleet/
│   │   ├── FleetOverview.tsx
│   │   └── AgentCard.tsx
│   ├── agent/
│   │   └── AgentDetail.tsx
│   ├── tasks/
│   │   └── TaskMonitor.tsx
│   ├── config/
│   │   └── ConfigEditor.tsx
│   └── audit/
│       └── AuditLog.tsx
├── hooks/
│   ├── useTheme.ts
│   ├── useSidebar.ts
│   └── useKeyboardShortcuts.ts
├── lib/
│   └── utils.ts         # cn() helper
├── App.tsx
├── main.tsx
└── index.css            # Tailwind + shadcn CSS variables
```

## Plan

- [ ] Initialize shadcn/ui: install CLI deps, generate `components.json`, add CSS variables to `index.css`
- [ ] Set up `cn()` utility (`lib/utils.ts`) with `clsx` + `tailwind-merge`
- [ ] Generate core shadcn/ui components: Button, Card, Badge, Table, Tabs, Dialog, AlertDialog, DropdownMenu, Tooltip, Sheet, ScrollArea, Skeleton, Command, Separator, Input, Select
- [ ] Implement `ThemeProvider` and theme toggle (Sun/Moon) with localStorage persistence
- [ ] Build `Layout` shell: sticky Header + collapsible Sidebar + ScrollArea content
- [ ] Integrate `sonner` toasts: wrap app in `<Toaster />`, add toast calls for agent lifecycle and config actions
- [ ] Refactor fleet overview to use shadcn Card, Badge, Table components
- [ ] Refactor agent detail view with Tabs, Skeleton loading states, AlertDialog for destructive actions
- [ ] Refactor task monitor and config editor views
- [ ] Refactor audit log view with Table, filtering, and empty state
- [ ] Add keyboard shortcut hooks (`Cmd+K` command palette, `Cmd+B` sidebar toggle)
- [ ] Add responsive behavior: sidebar auto-collapse, mobile Sheet nav
- [ ] Verify WCAG 2.1 AA: focus rings, ARIA labels, color contrast in both themes
- [ ] Update tests to cover theme toggle, sidebar collapse, toast display

## Test

- [ ] Dashboard renders without errors in both light and dark themes
- [ ] Theme toggle persists across page reloads
- [ ] Sidebar collapses/expands and state persists across reloads
- [ ] Toasts appear for agent start/stop/restart and config deploy actions
- [ ] `Cmd/Ctrl+K` opens command palette; `Cmd/Ctrl+B` toggles sidebar
- [ ] All views display loading skeletons while data is fetching
- [ ] Responsive layout: sidebar collapses on mobile, Sheet nav works
- [ ] No accessibility violations reported by axe-core in both themes
- [ ] All existing dashboard tests continue to pass