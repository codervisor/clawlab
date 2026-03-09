---
status: planned
created: 2026-03-09
priority: high
tags:
- core
- fleet
- auth
- security
- identity
- audit
- dashboard
depends_on:
- 061-local-agent-identity-auth
- 054-agent-fleet-execution-layer
- 025-llm-provider-api-key-management
parent: 060-agent-fleet-identity-auth
created_at: 2026-03-09T05:48:45.456961145Z
updated_at: 2026-03-09T05:48:45.456961145Z
---

# Operator Auth, Fleet RBAC & Audit — Human Access Controls

## Overview

Define how human operators authenticate to ClawDen and how auth-relevant activity is audited.

Agent auth alone is not enough. Once fleets can run with scoped identities, ClawDen also needs a clear operator model for local CLI use, remote dashboard/API access, token issuance, revocation, and audit visibility.

This child spec covers the human side of the auth system and the audit layer that makes scope decisions observable.

## Design

- Preserve the simple default for local development:
  - the OS user running the CLI is the human principal
- Add explicit auth flows for remote access:
  - session login/logout
  - bearer token creation and revocation
  - future OAuth-backed team login
- Define operator-facing scopes and admin boundaries for dashboard/API access.
- Persist auth events in the fleet audit store and expose them in CLI and dashboard surfaces.
- Log secret access, token issuance, revocation, and scope violations with actor identity.
- Show enough audit context to answer:
  - who accessed which secret
  - who granted or revoked access
  - which agent attempted an out-of-scope action

## Plan

- [ ] Define human principal auth methods and operator scope model.
- [ ] Add CLI flows for login, logout, token create, list, and revoke.
- [ ] Add server-side bearer-token validation for dashboard/API access.
- [ ] Persist auth and scope-violation events in the fleet audit store.
- [ ] Expose audit inspection via CLI and dashboard views.
- [ ] Document how local implicit auth differs from remote multi-user deployments.

## Test

- [ ] Local CLI use still works without extra login steps in single-user mode.
- [ ] Bearer token with insufficient scopes is rejected by the API.
- [ ] Revoked token no longer authorizes dashboard or API requests.
- [ ] Secret access and scope violation events are written to the audit store.
- [ ] Multi-user access rules prevent non-admin operators from performing admin-only actions.

## Notes

Keep this spec focused on human/operator auth and observability. Remote node enrollment and secret transport belong in the distributed child spec, not here.