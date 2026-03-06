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
depends_on:
- 012-fleet-orchestration
created_at: 2026-03-06T06:56:22.809363808Z
updated_at: 2026-03-06T06:56:22.809456972Z
---

# Agent Fleet Execution Layer — Master-Worker Orchestration, Message Bus & Task Lifecycle

> **Status**: planned · **Priority**: critical · **Created**: 2026-03-06

## Overview

ClawDen has solid data models for agent lifecycle, swarm coordination, and task routing (specs 011, 012), but the **execution layer** — actually running agents, passing messages between them, and collecting results — is not yet wired up. Everything is in-memory stubs.

This spec bridges the gap between ClawDen's orchestration API and real multi-agent execution. The goal: a human deploys a fleet of heterogeneous claw agents via `clawden.yaml`, defines master-worker relationships, and ClawDen handles process management, inter-agent messaging, task delegation, result aggregation, and persistent state — enabling a single individual to scale productivity through an agent fleet.

### Why Now

- The `ClawAdapter` trait, `LifecycleManager`, and `SwarmCoordinator` are implemented but return stubs
- No inter-agent communication exists — agents are isolated silos
- No task result flow — fan-out works but results don't flow back
- No persistence — fleet state is lost on restart
- This is the critical path from "orchestration framework" to "working agent fleet"

## Design

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        ClawDen Server                           │
│                                                                 │
│  ┌──────────────┐   ┌──────────────┐   ┌────────────────────┐  │
│  │  Fleet State  │   │  Message Bus │   │  Task Lifecycle    │  │
│  │  (SQLite)     │   │  (in-proc    │   │  Engine            │  │
│  │               │   │   channels)  │   │                    │  │
│  │  - agents     │   │              │   │  - decompose       │  │
│  │  - teams      │   │  - pub/sub   │   │  - delegate        │  │
│  │  - tasks      │   │  - request/  │   │  - collect         │  │
│  │  - results    │   │    reply     │   │  - aggregate       │  │
│  │  - audit log  │   │  - broadcast │   │  - report          │  │
│  └──────┬───────┘   └──────┬───────┘   └─────────┬──────────┘  │
│         │                  │                      │             │
│  ┌──────┴──────────────────┴──────────────────────┴──────────┐  │
│  │                   Process Supervisor                       │  │
│  │                                                            │  │
│  │  - Spawn agent processes (direct mode) or containers       │  │
│  │  - Attach stdin/stdout pipes for message passing           │  │
│  │  - Monitor health via adapter probes                       │  │
│  │  - Restart on failure with backoff                         │  │
│  └──┬──────────┬──────────┬──────────┬───────────────────────┘  │
│     │          │          │          │                           │
│  ┌──▼──┐   ┌──▼──┐   ┌──▼──┐   ┌──▼──┐                        │
│  │Agent│   │Agent│   │Agent│   │Agent│   ... N agents           │
│  │(OC) │   │(ZC) │   │(ZC) │   │(PC) │                        │
│  │Leader│   │Wrkr │   │Wrkr │   │Wrkr │                        │
│  └─────┘   └─────┘   └─────┘   └─────┘                        │
└─────────────────────────────────────────────────────────────────┘
```

### 1. Process Supervisor

Replace stub adapters with real execution. Two modes:

**Direct mode** (priority — no Docker dependency):
- Spawn runtime binary as a child process (`tokio::process::Command`)
- Attach stdin/stdout as a JSON-Lines message channel
- Generate runtime-native config into `~/.clawden/configs/<project>/<runtime>/`
- Capture stderr for log streaming

**Docker mode**:
- `docker run` via the existing `DockerAdapter`, but now with actual container management
- Attach via `docker exec` or HTTP API for message passing

The `ProcessSupervisor` wraps `ProcessManager` (already exists) with:
- Supervised restart with exponential backoff (reuse existing `backoff_ms`)
- Graceful shutdown (SIGTERM → wait → SIGKILL)
- Process group management for clean teardown of entire fleet

### 2. Inter-Agent Message Bus

In-process async message bus using `tokio::sync::broadcast` + `mpsc`:

```rust
pub struct MessageBus {
    /// Per-agent inbox: agent_id → mpsc::Sender<AgentEnvelope>
    inboxes: HashMap<String, mpsc::Sender<AgentEnvelope>>,
    /// Broadcast channel for fleet-wide events
    broadcast: broadcast::Sender<FleetEvent>,
}

pub struct AgentEnvelope {
    pub id: String,
    pub from: String,
    pub to: String,
    pub payload: MessagePayload,
    pub correlation_id: Option<String>,   // links request ↔ response
    pub timestamp: u64,
}

pub enum MessagePayload {
    /// Master → Worker: here's your assignment
    TaskAssignment { task_id: String, description: String, context: serde_json::Value },
    /// Worker → Master: here's my result
    TaskResult { task_id: String, result: serde_json::Value, status: TaskOutcome },
    /// Peer → Peer: freeform message
    Chat { role: String, content: String },
    /// System → Agent: health check, shutdown, config update
    System(SystemMessage),
}
```

Message delivery: the bus writes `AgentEnvelope` as JSON-Lines to each agent's stdin pipe. Agent responses come back via stdout, parsed by the supervisor, and routed through the bus.

### 3. Task Lifecycle Engine

Extends `SwarmCoordinator` with an execution-aware task state machine:

```
                    ┌──────────┐
         create     │ Created  │
                    └────┬─────┘
                         │ decompose
                    ┌────▼─────┐
                    │Delegated │──── fan-out to workers
                    └────┬─────┘
                         │ all workers ack
                    ┌────▼─────┐
                    │Executing │──── workers processing
                    └────┬─────┘
                    ┌────┴─────┐
              ┌─────▼──┐  ┌───▼────┐
              │Partial  │  │Complete│
              │Results  │  │Results │
              └────┬────┘  └───┬────┘
                   │           │ aggregate
                   │      ┌────▼─────┐
                   └─────▶│Aggregated│
                          └────┬─────┘
                               │ report to requester
                          ┌────▼─────┐
                          │  Done    │
                          └──────────┘
```

**Master-Worker pattern:**
1. Human (or master agent) submits a task via API or CLI
2. Task engine decomposes into subtasks based on team config
3. Subtasks are sent as `TaskAssignment` messages to workers via the bus
4. Workers process and return `TaskResult` messages
5. Engine aggregates results (configurable strategy: collect-all, first-wins, majority-vote)
6. Aggregated result returned to requester

### 4. Fleet Configuration (`clawden.yaml`)

```yaml
project: my-agent-fleet
mode: direct

agents:
  coordinator:
    runtime: openclaw
    model: claude-sonnet-4-20250514
    role: leader
    channels: [telegram]
    capabilities: [planning, code-review, delegation]

  coder-1:
    runtime: zeroclaw
    model: gpt-4.1
    role: worker
    capabilities: [code, test, refactor]

  coder-2:
    runtime: zeroclaw
    model: gpt-4.1
    role: worker
    capabilities: [code, test, refactor]

  researcher:
    runtime: picoclaw
    model: claude-sonnet-4-20250514
    role: worker
    capabilities: [web-search, summarize, research]

teams:
  dev-team:
    leader: coordinator
    workers: [coder-1, coder-2, researcher]
    strategy: collect-all    # or: first-wins, majority-vote

providers:
  anthropic:
    api_key: ${ANTHROPIC_API_KEY}
  openai:
    api_key: ${OPENAI_API_KEY}
```

`clawden up` reads this config, registers agents, creates teams, spawns processes, and starts the message bus. `clawden ps` shows live fleet status. `clawden logs <agent>` streams per-agent logs.

### 5. Persistent Fleet State (SQLite)

Replace in-memory `HashMap`s with SQLite via `rusqlite` (zero external deps):

| Table | Purpose |
|---|---|
| `agents` | Registration, state, capabilities, config |
| `teams` | Team definitions and membership |
| `tasks` | Task tree with parent-child relationships |
| `task_results` | Worker outputs linked to tasks |
| `messages` | Message log for debugging/replay |
| `audit_events` | Existing audit log, now persistent |

Location: `~/.clawden/state.db` (project-scoped via hash)

## Plan

- [ ] **Phase 1: Process Supervisor** — Wire up direct-mode process spawning for at least OpenClaw and ZeroClaw. Attach stdin/stdout pipes. Implement supervised restart.
- [ ] **Phase 2: Message Bus** — Implement `MessageBus` with tokio channels. Define `AgentEnvelope` wire format (JSON-Lines). Route messages between supervisor pipes and bus.
- [ ] **Phase 3: Task Lifecycle Engine** — Extend `SwarmCoordinator` with execution-aware states. Implement decompose → delegate → collect → aggregate flow.
- [ ] **Phase 4: Fleet Config** — Parse `agents:` and `teams:` sections from `clawden.yaml`. Wire `clawden up` to the full supervisor + bus + engine pipeline.
- [ ] **Phase 5: Persistence** — Add SQLite backend. Migrate agents, teams, tasks, messages from in-memory to persistent storage. Support fleet recovery on restart.
- [ ] **Phase 6: CLI Polish** — `clawden ps` with live fleet table, `clawden logs <agent>` streaming, `clawden send <agent> <message>` for ad-hoc communication.

## Test

- [ ] Spawn 2+ agents in direct mode, verify they start and respond to health checks
- [ ] Send a `TaskAssignment` from leader to worker via message bus, verify `TaskResult` comes back
- [ ] Full fan-out: submit a task to a team, verify all workers receive subtasks and results aggregate
- [ ] Kill a worker process, verify supervisor restarts it with backoff
- [ ] Restart ClawDen server, verify fleet state persists and agents are recoverable
- [ ] `clawden up` from a `clawden.yaml` with 3+ agents, verify entire fleet comes online

## Notes

### Alternatives Considered

- **gRPC between agents**: Too heavy for v1. JSON-Lines over stdin/stdout is simpler, works cross-platform, and doesn't require agents to run a server. Can upgrade to gRPC/WebSocket later.
- **Redis/NATS message bus**: Adds external dependency. In-process tokio channels are sufficient for single-host fleets. When multi-host is needed, swap the bus backend without changing the API.
- **PostgreSQL for state**: Overkill for personal/small-team use. SQLite is zero-config, embedded, and handles the expected scale (dozens of agents, thousands of tasks).

### Open Questions

- Should the message bus support cross-host routing (e.g., agent on laptop talks to agent on VPS)? Defer to a future spec — start single-host.
- What's the protocol for agent capability negotiation at startup? (Agent connects → declares capabilities → bus routes accordingly)
- Should results aggregation be pluggable? (e.g., custom Rust/WASM aggregators for domain-specific merge logic)
