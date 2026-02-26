---
status: in-progress
created: 2026-02-26
priority: high
tags:
- core
- fleet
- routing
- swarm
depends_on:
- 010-claw-runtime-interface
parent: 009-orchestration-platform
created_at: 2026-02-26T02:08:29.575833924Z
updated_at: 2026-02-26T03:26:22.415306720Z
transitions:
- status: in-progress
  at: 2026-02-26T03:26:22.415306720Z
---
# Fleet Discovery & Task Routing

## Overview

Fleet orchestration handles agent discovery, registration, and intelligent task routing. It enables multi-agent collaboration by routing tasks to the most appropriate claw agent based on capabilities, hardware constraints, current load, and cost.

## Design

### Agent Registry
Each agent registers with capabilities metadata:
```typescript
interface AgentRegistration {
  id: string;
  runtime: string;        // "openclaw" | "zeroclaw" | "picoclaw" | ...
  version: string;
  host: string;
  capabilities: string[]; // ["code", "web-search", "memory", "voice", ...]
  hardware: HardwareProfile; // cpu, ram, arch, cost-tier
  channels: string[];     // ["telegram", "discord", "whatsapp", ...]
  status: AgentStatus;
  load: LoadMetrics;      // current utilization
}
```

### Task Routing Strategies
- **Capability match**: Route to agents that have required tools/skills
- **Hardware fit**: Match task resource requirements to agent hardware
- **Load balancing**: Spread tasks across available agents
- **Cost optimization**: Prefer cheaper runtimes when capability is equal
- **Affinity**: Sticky sessions for stateful conversations

### Swarm Coordination
- Define agent teams with roles (leader, worker, reviewer)
- Task decomposition and fan-out to team members
- Result aggregation and consensus
- Cross-runtime communication via ClawDen message bus

## Plan

- [x] Implement agent registry with registration/deregistration
- [x] Build capability-based task router
- [x] Add load balancing and cost-aware routing
- [ ] Implement agent discovery (network scan, manual, DNS-SD)
- [ ] Design swarm coordination protocol
- [ ] Create REST/WebSocket APIs for fleet management

## Test

- [x] Agents can register and be discovered
- [x] Task router selects correct agent based on capabilities
- [ ] Load balancer distributes tasks evenly
- [ ] Swarm can coordinate a multi-step task across runtimes