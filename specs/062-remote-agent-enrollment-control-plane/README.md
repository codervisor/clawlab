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
- distributed
depends_on:
- 061-local-agent-identity-auth
- 054-agent-fleet-execution-layer
- 053-agent-workspace-persistence
parent: 060-agent-fleet-identity-auth
created_at: 2026-03-09T05:48:45.442971492Z
updated_at: 2026-03-09T05:48:45.442971492Z
---

# Remote Fleet Enrollment & Secret Delivery — Production Distributed Auth

## Overview

Extend the local auth foundation to support production fleet deployments where agents run on separate hosts.

This child spec covers the distributed path: enrollment, node identity, sealed secret delivery, token refresh, revocation, and the secure control channel between remote nodes and the fleet control plane.

The design assumption is explicit: local fleets are the fast dev and POC path, while remote fleets are the production architecture.

## Design

- Add one-time enrollment flow:
  - control plane issues an enrollment token for a named agent
  - remote host runs `clawden agent join`
  - remote node generates a keypair and registers its public key
- Bind agent identity to a specific node so stolen tokens cannot be replayed from another host.
- Deliver remote secrets as sealed envelopes encrypted to the node public key.
- Store remote secrets in memory-backed storage when available and re-seal on rotation.
- Add persistent outbound control channel from remote node to control plane for:
  - heartbeat
  - token refresh
  - secret rotation
  - revocation
  - message relay
- Extend fleet config with control-plane and node assignment fields for distributed topologies.
- Define partition behavior so agents degrade safely during control-plane outages.

## Plan

- [ ] Implement enrollment-token issuance, expiry, revocation, and single-use claim semantics.
- [ ] Add remote node key generation and registration during `agent join`.
- [ ] Implement sealed-envelope secret delivery and remote unseal flow.
- [ ] Build control-plane auth channel for heartbeat, refresh, revoke, and relay events.
- [ ] Add distributed config schema for `fleet.control_plane`, `fleet.nodes`, and `agents.*.node`.
- [ ] Implement partition grace-period behavior and remote secret wipe/revocation handling.

## Test

- [ ] Valid enrollment token can be claimed exactly once.
- [ ] Expired or reused enrollment token is rejected.
- [ ] Sealed secrets cannot be decrypted without the enrolled node key.
- [ ] Remote agent refreshes identity before expiry without task interruption.
- [ ] Revocation reaches remote nodes within the configured heartbeat window.
- [ ] Partition policy transitions agent from healthy to degraded to suspended as configured.
- [ ] Remote agents on multiple hosts can exchange messages only through authenticated relay paths.

## Notes

This spec should build directly on the primitives from spec 061. If the local identity model changes, this spec should reuse it rather than layering a separate remote-only model on top.