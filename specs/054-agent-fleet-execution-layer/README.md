---
status: planned
created: 2026-03-06
priority: critical
tags:
- core
- fleet
- orchestration
- message-bus
- master-worker
- ai-native
- coordination
- cost-optimization
depends_on:
- 012-fleet-orchestration
created_at: 2026-03-06T06:56:22.809363808Z
updated_at: 2026-03-06T06:56:22.809456972Z
---
# Agent Fleet Execution Layer — Master-Worker Orchestration, Message Bus & Task Lifecycle

## Overview

This spec is the umbrella for the fleet execution layer — running agents, passing messages between them, collecting results, persisting state, and coordinating sophisticated multi-agent workflows.

The work splits into five layers that build on each other:

1. **Execution substrate** — agents running and staying alive.
2. **Collaboration protocol** — agents communicating and working together on tasks.
3. **Reliability layer** — fleet state surviving crashes and restarts.
4. **Coordination intelligence** — advanced and AI-native coordination patterns that go beyond simple master-worker.
5. **Cost optimization** — teacher-student knowledge distillation to reduce fleet operating costs.

Layers 1–3 are single-host by design. Distributed execution (cross-host message relay, remote supervisor) builds on top of spec 062's control channel, reusing the same `AgentEnvelope` protocol and supervisor interface. Layers 4–5 are transport-agnostic and work identically on local and distributed fleets.

## Design

This umbrella coordinates seven child specs across the five layers:

### Layer 1–3: Execution Foundation

| Child | Purpose |
| --- | --- |
| `064-fleet-process-supervisor` | Spawn agents, attach pipes, health probes, supervised restart, graceful shutdown, fleet config parsing, `clawden up` |
| `065-agent-message-bus-task-orchestration` | In-process message bus, `AgentEnvelope` protocol, team coordination, task lifecycle engine, result aggregation |
| `066-fleet-state-persistence-recovery` | SQLite backend for agents/teams/tasks/results/messages/audit, crash recovery, `clawden logs`/`clawden audit` |

### Layer 4: Coordination Intelligence

| Child | Purpose |
| --- | --- |
| `067-advanced-coordination-patterns` | Pluggable org-chart patterns: hierarchy, pipeline, committee, marketplace — mapping real-world organizational structures onto agent fleets |
| `068-ai-native-coordination-primitives` | Primitives with no human analogue: speculative swarm, context mesh, fractal decomposition, generative-adversarial, stigmergic — exploiting zero fork cost, lossless context transfer, and speculative parallelism |
| `069-ai-native-domain-playbooks` | Applied compositions of AI-native primitives for concrete domains: software engineering, finance, marketing, research, legal, devops |
| `070-sdd-ai-native-playbook` | AI-native playbooks applied to spec-driven development itself — spec exploration, hardening, living graph maintenance, fractal decomposition |

### Layer 5: Cost Optimization

| Child | Purpose |
| --- | --- |
| `071-nemosis-teacher-student-distillation` | Nemosis integration for teacher-student knowledge distillation — captures fleet execution traces, distills them into SKILL.md artifacts, routes subsequent runs to cheaper student models with iterative memory-backed refinement |

Shared architectural rules:

- JSON-Lines over stdin/stdout is the agent communication wire format.
- `AgentEnvelope` is the stable message protocol used by both local and (future) remote delivery.
- The process supervisor owns agent lifecycle; the message bus owns routing; persistence is the durability layer underneath both.
- Master-worker is the foundational collaboration pattern; advanced and AI-native patterns (067–070) are pluggable coordination strategies on top of the same bus.
- AI-native primitives (068) extend the coordination trait surface with `spawn`, `merge`, `fork`, `observe`, `convergence`, and `prune` operations that exploit properties unique to AI agents.
- Nemosis (071) operates as a sidecar that captures traces, distills skills, and informs the scheduler's model selection — reducing fleet cost by 50–90% for repetitive patterns.

## Plan

- [ ] Complete spec 064 to establish agent process management and fleet startup.
- [ ] Complete spec 065 to add inter-agent messaging and task orchestration on top of the running fleet.
- [ ] Complete spec 066 to make fleet state persistent and recoverable.
- [ ] Complete spec 067 to add pluggable organizational coordination patterns.
- [ ] Complete spec 068 to define AI-native coordination primitives.
- [ ] Complete specs 069–070 to map primitives to domain and SDD playbooks.
- [ ] Complete spec 071 to integrate Nemosis for fleet cost optimization via distillation.

## Test

- [ ] A fleet of 3+ heterogeneous agents starts, stays healthy, and shuts down cleanly.
- [ ] A master-worker task flow produces aggregated results from multiple workers.
- [ ] Fleet state survives a crash and resumes on restart.
- [ ] Advanced coordination patterns (hierarchy, pipeline, committee) produce correct results via the same message bus.
- [ ] AI-native primitives (speculative swarm, context mesh) produce outputs no single agent could achieve alone.
- [ ] Nemosis distillation reduces fleet cost by routing repetitive agent roles to student models without quality degradation.

## Notes

Implementation order for the foundation is strictly sequential: 064 → 065 → 066. Each layer depends on the previous one.

The coordination intelligence layer (067–070) builds on the foundation but is internally layered: 067 (org-chart patterns) → 068 (AI-native primitives) → 069 (domain playbooks) → 070 (SDD playbook). Each extends the previous.

The cost optimization layer (071) depends on the coordination primitives (068–069) and hooks into all three foundation layers: trace capture via the process supervisor (064), message observation via the bus (065), and trace persistence via the SQLite backend (066).

The distributed story connects here:
- Spec 062 (remote enrollment + control channel) provides the transport for cross-host message relay.
- Spec 065's `AgentEnvelope` format is the protocol that travels over that transport.
- A future spec can add a `RemoteMessageBus` backend that routes envelopes through 062's control channel, swapping the tokio channel backend without changing the bus API.