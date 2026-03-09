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
parent: 060-agent-fleet-identity-auth
created_at: 2026-03-09T05:48:45.416147735Z
updated_at: 2026-03-09T05:48:45.416147735Z
---

# Local Fleet Identity & Scoped Secrets — Dev/POC Foundation

## Overview

Establish the single-host authentication and authorization foundation for ClawDen fleets.

This child spec intentionally targets the simplest deployment shape first: all agents run as local child processes on the same machine as the control plane. That gives us a practical dev and POC path without dragging in remote enrollment, mTLS, or distributed control-channel complexity on day one.

The goal is to stop treating all local agents as equally trusted. Each agent should have a typed identity, a bounded scope set, and access only to its own secrets plus explicitly shared secrets.

## Design

- Introduce shared auth primitives in `clawden-core`: `Principal`, `AgentRole`, scope types, and local identity token claims.
- Replace the flat local secret model with a per-principal vault layout on the control-plane host:
  - `human/`
  - `agents/<agent-id>/`
  - `shared/`
- Extend fleet config so each agent can declare:
  - `role`
  - `scopes`
  - `secrets`
- Add role defaults under `fleet.auth.role_defaults` so common local fleets stay concise.
- Update the local process supervisor to inject only:
  - the agent's own secrets
  - shared secrets
  - the agent's identity token
- Authenticate local inter-agent messages using signed identity tokens instead of trusting process-local callers implicitly.
- Non-goals for this child:
  - remote node enrollment
  - sealed secret delivery
  - mTLS control channels
  - dashboard bearer auth

## Plan

- [ ] Define core identity and scope types shared by fleet components.
- [ ] Implement per-principal local vault layout and migration path from flat secrets.
- [ ] Add local-only config schema for agent roles, scopes, and secret bindings.
- [ ] Update local supervisor startup to scope env injection per agent.
- [ ] Require identity-token verification on local fleet message send/receive paths.
- [ ] Add local CLI and runtime errors for scope violations and missing scoped secrets.

## Test

- [ ] Local agent process receives only its own secrets and shared secrets.
- [ ] Human-only secrets are never mounted into agent processes.
- [ ] Agent without `message-send` permission cannot message a denied peer.
- [ ] Agent without a tool scope cannot invoke the blocked tool.
- [ ] Identity tokens rotate without local agent downtime.
- [ ] Existing single-host fleet flows remain usable with minimal config overhead.

## Notes

This spec is the required base for the production/distributed work. Remote agents should reuse the same identity and scope model rather than inventing a second auth system.