---
status: planned
created: 2026-03-09
priority: critical
tags:
- core
- fleet
- auth
- security
- identity
- secrets
depends_on:
- 054-agent-fleet-execution-layer
- 025-llm-provider-api-key-management
- 053-agent-workspace-persistence
created_at: 2026-03-09T03:11:29.260307188Z
updated_at: 2026-03-09T03:11:29.260307188Z
---
# Agent Fleet Identity & Authorization — Secure Multi-Agent Auth Architecture

## Overview

This spec is now the umbrella for ClawDen fleet identity, authentication, and authorization.

The original draft was too large to implement and review as a single unit. The work naturally splits into a local-first foundation and the production distributed path:

- Local auth is the fast dev and POC path.
- Remote auth is the production path for real multi-host fleets.

The core problem remains unchanged: ClawDen must stop treating every agent as equally trusted and must stop conflating human credentials with agent credentials.

## Design

This umbrella coordinates three child specs:

| Child | Purpose |
| --- | --- |
| `061-local-agent-identity-auth` | Single-host identity, scoped secrets, and local message auth for dev/POC fleets |
| `062-remote-agent-enrollment-control-plane` | Remote enrollment, sealed secret delivery, revocation, and control-plane auth for production fleets |
| `063-operator-auth-audit-rbac` | Human/operator auth, dashboard/API access control, and audit visibility |

Shared architectural rules across all children:

- Every principal is typed: human, agent, or system.
- Secrets are scoped per principal, with explicit shared secrets when needed.
- Local and remote agents reuse the same identity and scope model.
- Human credentials are never implicitly inherited by agents.
- Auth-relevant actions are auditable.

## Plan

- [ ] Complete spec 061 to establish local identity, scope, and secret-isolation primitives.
- [ ] Complete spec 062 to extend those primitives to remote nodes and production topology.
- [ ] Complete spec 063 to secure human/operator access and make auth decisions observable.
- [ ] Reconcile any cross-cutting config or schema changes at the umbrella level once child designs stabilize.

## Test

- [ ] A local single-host fleet can run with per-agent scoped secrets and no cross-agent secret leakage.
- [ ] A remote multi-host fleet can enroll agents, deliver secrets securely, rotate credentials, and revoke access.
- [ ] Human operators can authenticate with appropriate scopes and view audit history.
- [ ] The same identity and scope model applies consistently across local and remote deployments.

## Notes

Implementation order should be:

1. Local foundation first.
2. Remote production path second.
3. Operator auth and audit surfaces alongside or immediately after the shared auth primitives are stable.

That sequencing keeps the first usable milestone small while still preserving the full production architecture.