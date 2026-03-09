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

ClawDen's fleet layer (spec 054) handles execution — spawning agents, routing messages, collecting results. But it treats all agents as equally trusted and conflates human credentials with agent credentials. When a human deploys a fleet of AI agents, critical questions are unanswered:

- **Who owns which secrets?** If the coordinator agent has an Anthropic API key and a GitHub PAT, should the worker agents also have access to those?
- **How do agents authenticate to external services?** Each agent might need different scopes — one agent codes (needs repo write), another researches (needs read-only web access).
- **How do humans authenticate to ClawDen itself?** Multiple humans may manage the same fleet with different permission levels.
- **What stops a compromised agent from escalating?** If a worker agent is jailbroken, it shouldn't be able to read the coordinator's secrets or impersonate a human.

This spec defines the identity, authentication, and authorization architecture for ClawDen fleets — treating agents as managed employees with scoped credentials, not as extensions of the human operator.

### The Employee Metaphor

Think of a ClawDen fleet as a small company:

| Concept | Company Analogy | ClawDen Equivalent |
|---|---|---|
| **LLM** | Employee's brain & intelligence | Model provider (OpenAI, Anthropic, etc.) |
| **Memory system** | Employee's notes, knowledge, experience | Workspace persistence (spec 053) — git-backed memory |
| **Claw runtime** | Employee's workstation (laptop, desk, tools) | OpenClaw/ZeroClaw/PicoClaw process + config |
| **Identity** | Employee badge / corporate ID | Agent identity token issued by ClawDen |
| **Credentials** | Keycards, passwords, access badges per system | Scoped secrets vaulted per agent |
| **Role** | Job title & responsibilities | Agent role (leader, worker, reviewer) |
| **Clearance level** | Security clearance tier | Permission scope (what secrets/tools/channels the agent can access) |
| **HR / IT department** | Issues badges, manages access, revokes on termination | ClawDen control plane — the fleet auth manager |
| **Office / branch** | Physical location where employee works | Host infrastructure — Codespace, VPS, cloud VM, edge device |
| **VPN / corporate network** | Secure tunnel between offices | Encrypted control channel (mTLS / WireGuard) between fleet nodes |
| **Badge reader at the door** | Verifies employee belongs before granting building access | Enrollment protocol — agent proves it was invited before receiving secrets |

Just as a company doesn't give every employee the CEO's email password, ClawDen shouldn't give every agent the human operator's full credential set. And just as a company with remote offices doesn't ship keycards via postcard — ClawDen doesn't send secrets to remote agents in plaintext over the network.

### Why Now

- Spec 054 is about to wire up real multi-agent execution — agents will actually run side-by-side
- Spec 025 added provider API keys but they're fleet-global — every agent sees every key
- Spec 053 gives agents persistent memory — if an agent is compromised, its memory repo contains sensitive context
- Without auth boundaries, a single compromised agent = full fleet compromise
- This is the difference between "running multiple chatbots" and "operating aligned AI employees"
- Real-world agents are **distributed** — running on different Codespaces, VPSes, cloud VMs, or edge devices. Auth can't assume a shared filesystem or a single host. The control plane must deliver credentials over the network, verify agent identity remotely, and revoke access across infrastructure boundaries.

## Design

### 1. Identity Model

Every entity in ClawDen gets a typed identity:

```rust
pub enum Principal {
    /// Human operator — owns the fleet, manages config
    Human {
        id: String,
        name: String,
        auth_method: HumanAuthMethod,
    },
    /// AI agent — a managed employee in the fleet
    Agent {
        id: String,          // stable across restarts: "coordinator", "coder-1"
        runtime: String,     // "openclaw", "zeroclaw", etc.
        role: AgentRole,
        team: Option<String>,
    },
    /// ClawDen system — internal operations (sync, health checks)
    System,
}

pub enum HumanAuthMethod {
    /// Local CLI — authenticated by OS user (single-user default)
    LocalSession,
    /// Token-based — for remote dashboard / API access
    BearerToken { token_hash: String, scopes: Vec<Scope> },
    /// OAuth — GitHub, Google SSO for team deployments
    OAuth { provider: String, subject: String },
}

pub enum AgentRole {
    Leader,      // can delegate tasks, see team-wide context
    Worker,      // executes assigned tasks within scope
    Reviewer,    // read-only + approval authority
    Specialist,  // worker with elevated access to specific tools/services
}
```

### 2. Scoped Secret Vault

Replace the current flat secret store with a per-principal vault — designed to work both locally and across networks.

#### Control Plane Vault (on the fleet manager's host)

```
~/.clawden/vault/
├── fleet.key              # Master key (encrypted with human's passphrase or OS keychain)
├── fleet.pub              # Public key — distributed to agent nodes for envelope encryption
├── human/
│   └── default/           # Human operator's secrets
│       ├── github-pat      # Full-scope PAT (human only)
│       └── anthropic-key   # Personal API key
├── agents/
│   ├── coordinator/
│   │   ├── anthropic-key   # Coordinator's own API key (or delegated subset)
│   │   ├── github-pat      # Scoped: repo read + issue write only
│   │   └── telegram-token  # Coordinator owns the Telegram channel
│   ├── coder-1/
│   │   ├── openai-key      # Worker's LLM key
│   │   └── github-pat      # Scoped: repo read + PR create only
│   └── researcher/
│       ├── anthropic-key
│       └── web-search-key  # Only researcher has web search API access
├── shared/                 # Secrets explicitly shared across the fleet
│   └── sentry-dsn          # Error reporting — all agents can read
└── enrollment/
    ├── pending/            # One-time enrollment tokens awaiting claim
    └── enrolled/           # Agent node registrations (host fingerprint, public key)
```

#### Agent-Side Vault (on each remote host)

Remote agents don't see the full vault — they receive a sealed envelope at enrollment:

```
~/.clawden/agent-vault/
├── identity.jwt           # Agent's current identity token
├── node.key               # Agent node's keypair (generated at enrollment)
├── fleet.pub              # Fleet's public key (for verifying control plane messages)
└── secrets/               # Decrypted secrets (from sealed envelope, in tmpfs/memory when possible)
    ├── anthropic-key
    └── github-pat
```

**Key principles:**
- **Default deny**: an agent can only access secrets in its own vault directory + `shared/`
- **Human escalation**: agents cannot access `human/` secrets — ever
- **Delegation, not sharing**: when a human wants an agent to have a GitHub PAT, they issue a *scoped* token and store it in the agent's vault — not copy their own
- **Rotation-friendly**: each secret has metadata (created_at, expires_at, source) for auditing
- **Sealed delivery**: secrets travel over the network as sealed envelopes encrypted to the agent's node key — the control plane can push secret updates without the transport layer seeing plaintext
- **Ephemeral on remote hosts**: agent-side secrets live in memory-backed storage (tmpfs) when available — if the host is compromised, rebooting clears the secrets

### 3. Permission Scopes

Fine-grained capabilities that bound what an agent can do:

```rust
pub enum Scope {
    // Secret access
    SecretRead(SecretPattern),     // "anthropic-key", "github-*"
    SecretWrite(SecretPattern),    // can store new secrets (e.g., OAuth tokens obtained during work)
    SharedSecretRead,              // access shared/ vault

    // Fleet interaction
    MessageSend(AgentPattern),     // who this agent can message
    MessageReceive(AgentPattern),  // who can message this agent
    TaskDelegate(AgentPattern),    // who this agent can assign tasks to
    TaskView,                      // can see fleet-wide task board

    // External access
    ToolUse(ToolPattern),          // which tools the agent can invoke
    ChannelAccess(ChannelPattern), // which channels the agent can interact with
    NetworkAccess(NetworkPolicy),  // outbound network restrictions

    // System
    ConfigRead,                    // can read clawden.yaml (sanitized)
    ConfigWrite,                   // can modify fleet config (leader only)
    AuditRead,                     // can read audit logs
}
```

### 4. Agent Credential Lifecycle

Two paths: **local agents** (child processes on the same host) and **remote agents** (on other machines).

#### Local Agent Flow (same host as control plane)

```
Human deploys fleet
        │
        ▼
┌───────────────────┐
│ clawden up         │
│                    │
│  For each agent:   │
│  1. Generate agent │◄── Agent gets a unique identity token
│     identity token │    (JWT signed by fleet.key)
│  2. Resolve scopes │◄── From role + clawden.yaml overrides
│  3. Mount secrets  │◄── Only agent's vault dir + shared/
│  4. Inject env     │◄── Secrets as env vars into runtime process
│  5. Start process  │
└───────────────────┘
        │
        ▼
┌───────────────────┐
│ Agent runtime      │
│                    │
│ Sees only:         │
│ - Own API keys     │
│ - Own channel tkns │
│ - Shared secrets   │
│ - Identity token   │
│                    │
│ Cannot see:        │
│ - Human's secrets  │
│ - Other agents' keys│
│ - Fleet master key │
└───────────────────┘
```

#### Remote Agent Flow (different host — Codespace, VPS, cloud VM)

```
┌─────────────────────────────────────────────────────────────────────┐
│ CONTROL PLANE HOST (human's machine or fleet server)               │
│                                                                     │
│  1. Human runs: clawden fleet enroll coder-1                        │
│     → Generates one-time enrollment token (OTT)                     │
│     → Prints: clawden agent join <fleet-url> --token <OTT>          │
│                                                                     │
│  4. Control plane receives enrollment request                       │
│     → Verifies OTT (single-use, expires in 10min)                   │
│     → Records agent's node public key                               │
│     → Seals agent's secrets with agent's public key                 │
│     → Signs identity JWT                                            │
│     → Returns sealed envelope + JWT over TLS                        │
│                                                                     │
│  7. Periodic: push secret rotations as re-sealed envelopes          │
│     Periodic: verify agent liveness via heartbeat                   │
│     On revoke: broadcast revocation to all fleet nodes              │
└─────────────────────────────────────────────────────────────────────┘
                              │
                         TLS / mTLS
                              │
┌─────────────────────────────────────────────────────────────────────┐
│ REMOTE AGENT HOST (Codespace, VPS, cloud VM)                        │
│                                                                     │
│  2. Human (or CI) runs on remote host:                              │
│     clawden agent join https://fleet.example.com --token <OTT>      │
│     → Generates node keypair (node.key / node.pub)                  │
│     → Sends enrollment request with node.pub to control plane       │
│                                                                     │
│  3. Agent stores fleet.pub for verifying future control messages     │
│                                                                     │
│  5. Agent unseals envelope with node.key                            │
│     → Secrets written to tmpfs (~/.clawden/agent-vault/secrets/)    │
│     → Identity JWT stored for message bus auth                      │
│                                                                     │
│  6. Agent runtime starts with secrets as env vars                   │
│     → Heartbeats to control plane on interval                       │
│     → Accepts re-sealed envelopes for secret rotation               │
└─────────────────────────────────────────────────────────────────────┘
```

#### Identity Tokens

**Identity tokens** are short-lived JWTs signed by the fleet master key:
- Contains: agent_id, role, scopes, team, node_id, issued_at, expires_at
- Refreshed automatically by the process supervisor (local) or via control channel (remote)
- Used for message bus authentication (agents prove identity when sending messages)
- Revoked immediately on agent stop/decommission — revocation list propagated to all nodes
- `node_id` binds the token to a specific host — prevents token theft + replay from another machine

### 5. Fleet Configuration Extension

```yaml
# clawden.yaml — single-host fleet (simplest case, no distributed overhead)
fleet:
  auth:
    # How secrets are encrypted at rest
    vault_backend: os-keychain  # or: passphrase, age-encryption, vault-server
    # Default scopes for each role
    role_defaults:
      leader:
        scopes: [secret-read:*, message-send:*, task-delegate:*, config-read, audit-read]
      worker:
        scopes: [secret-read:own, message-send:leader, tool-use:*]
      reviewer:
        scopes: [secret-read:own, message-receive:*, task-view, audit-read]

agents:
  coordinator:
    runtime: openclaw
    role: leader
    # Per-agent scope overrides
    scopes:
      - channel-access:telegram
      - secret-read:shared/*
    secrets:
      anthropic-key: $COORDINATOR_ANTHROPIC_KEY
      github-pat: $COORDINATOR_GITHUB_TOKEN
      telegram-token: $TELEGRAM_BOT_TOKEN

  coder-1:
    runtime: zeroclaw
    role: worker
    scopes:
      - tool-use:git,code,test
      - network-access:github.com,npmjs.com
    secrets:
      openai-key: $CODER_OPENAI_KEY
      github-pat: $CODER_GITHUB_TOKEN  # scoped: repo read + PR create

  researcher:
    runtime: picoclaw
    role: specialist
    scopes:
      - tool-use:web-search,summarize
      - network-access:*              # researcher needs broad web access
    secrets:
      anthropic-key: $RESEARCHER_ANTHROPIC_KEY
      web-search-key: $TAVILY_API_KEY
```

Note: for distributed fleet config with `fleet.nodes`, `fleet.control_plane`, and per-agent `node:` assignments, see **section 9: Distributed Fleet Topology**.

### 6. Human Authentication

For single-user local deployments (the common case), auth is implicit — the OS user running `clawden` CLI is the human principal. For multi-user or remote access:

**Dashboard / API access:**
- `clawden auth login` — generates a session token stored in OS keychain
- `clawden auth token create --scopes admin` — creates a long-lived API token
- Dashboard uses bearer token auth against the ClawDen server
- Tokens are revocable via `clawden auth token revoke <id>`

**Team deployments (future):**
- GitHub OAuth — `clawden auth login --provider github`
- Map GitHub org membership to fleet permissions
- Org admin = fleet admin, org member = read-only dashboard

### 7. Threat Model

| Threat | Mitigation |
|---|---|
| Compromised worker agent reads other agents' API keys | Per-agent vault isolation — process-level env var scoping; remote agents never see other agents' envelopes |
| Jailbroken agent sends messages as another agent | Message bus requires JWT identity verification; JWT bound to node_id |
| Agent exfiltrates secrets via LLM conversation | Network access scoping; audit log on secret reads |
| Stolen fleet.key decrypts all secrets | OS keychain or hardware key storage; passphrase-protected fallback |
| Dashboard session hijacking | Short-lived tokens, CSRF protection, SameSite cookies |
| Replay attack on agent identity token | JWT expiry + nonce + node_id binding; supervisor rotates tokens |
| Human's personal PAT leaked via agent | Human vault is never mounted into agent processes |
| **Network eavesdropping on secret delivery** | Sealed envelope encryption (agent's node key) inside TLS — double encryption |
| **Enrollment token theft** | One-time use, 10-minute expiry, IP-binding optional; stolen OTT usable only once |
| **Compromised remote host** | Secrets in tmpfs (cleared on reboot); node key revocation propagates immediately; no fleet.key on remote hosts |
| **Man-in-the-middle on control channel** | mTLS after enrollment; certificate pinning to fleet.pub; reject unknown CAs |
| **Rogue agent joins fleet** | Enrollment requires OTT issued by human; no auto-discovery of fleet endpoint |
| **Split-brain after network partition** | Agents operate with cached credentials but cannot refresh; configurable grace period before auto-shutdown |
| **Agent moves between hosts (IP change)** | Identity bound to node keypair, not IP; re-enrollment required for new host |

### 8. Audit Trail

All auth-relevant events are logged to the persistent audit store (spec 054's SQLite):

```rust
pub enum AuthAuditEvent {
    HumanLogin { method: HumanAuthMethod, ip: Option<String> },
    TokenCreated { principal: Principal, scopes: Vec<Scope>, expires_at: u64 },
    TokenRevoked { token_id: String, revoked_by: Principal },
    SecretAccessed { principal: Principal, secret_path: String },
    SecretRotated { secret_path: String, rotated_by: Principal },
    ScopeViolation { principal: Principal, attempted: Scope, denied_reason: String },
    AgentIdentityIssued { agent_id: String, scopes: Vec<Scope> },
    AgentDecommissioned { agent_id: String, secrets_purged: bool },
}
```

Every `ScopeViolation` is logged and optionally alerts the human operator — this is how you know an agent tried to do something outside its role.

### 9. Distributed Fleet Topology

Real-world fleets are not single-host. Agents run on separate infrastructure — Codespaces, cloud VMs, VPS instances, edge devices, corporate laptops behind NAT. This changes everything about how identity, secrets, and communication work.

#### Network Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        CONTROL PLANE                                    │
│                   (human's machine or fleet server)                      │
│                                                                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌────────────┐  │
│  │ Vault        │  │ Enrollment   │  │ Message      │  │ Audit      │  │
│  │ (master keys,│  │ Service      │  │ Router       │  │ Aggregator │  │
│  │  all agent   │  │ (OTT issue,  │  │ (cross-node  │  │ (collects  │  │
│  │  secrets)    │  │  key exchange│  │  message     │  │  events    │  │
│  │              │  │  revocation) │  │  relay)      │  │  from all  │  │
│  │              │  │              │  │              │  │  nodes)    │  │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └──────┬─────┘  │
│         └─────────────────┴─────────────────┴─────────────────┘        │
│                              │ Fleet API (HTTPS + mTLS)                 │
└──────────────────────────────┼──────────────────────────────────────────┘
                               │
            ┌──────────────────┼──────────────────┐
            │                  │                  │
    ┌───────▼───────┐  ┌───────▼───────┐  ┌───────▼───────┐
    │ AGENT NODE 1  │  │ AGENT NODE 2  │  │ AGENT NODE 3  │
    │ (Codespace)   │  │ (VPS)         │  │ (Cloud VM)    │
    │               │  │               │  │               │
    │ coordinator   │  │ coder-1       │  │ researcher    │
    │ (openclaw)    │  │ (zeroclaw)    │  │ (picoclaw)    │
    │               │  │               │  │               │
    │ agent-vault/  │  │ agent-vault/  │  │ agent-vault/  │
    │  identity.jwt │  │  identity.jwt │  │  identity.jwt │
    │  node.key     │  │  node.key     │  │  node.key     │
    │  secrets/     │  │  secrets/     │  │  secrets/     │
    └───────────────┘  └───────────────┘  └───────────────┘
```

#### Enrollment Protocol

The enrollment flow follows the "badge issuing" metaphor. Like an HR department issuing building access to a new employee at a remote office:

```
Step 1 — Invitation (control plane side):
  $ clawden fleet enroll coder-1 --node vps-amsterdam
  Enrollment token (valid 10 min): cldn_enroll_1a2b3c4d5e6f
  Run this on the remote host:
    clawden agent join https://fleet.example.com --token cldn_enroll_1a2b3c4d5e6f

Step 2 — Claim (remote host side):
  $ clawden agent join https://fleet.example.com --token cldn_enroll_1a2b3c4d5e6f
  → Generates node keypair (Ed25519)
  → Sends enrollment request: { agent_id, node_pub, host_fingerprint }
  → Control plane verifies OTT, records node, seals secrets
  → Agent receives: sealed_envelope + identity.jwt + fleet.pub
  → Secrets unsealed and stored in tmpfs
  ✓ Enrolled as "coder-1" on node "vps-amsterdam"

Step 3 — Operational:
  → Agent sends heartbeat every 60s (configurable)
  → Control plane pushes secret rotations as re-sealed envelopes
  → Message bus routes cross-node messages through the control plane relay
  → Audit events streamed to control plane aggregator
```

**Enrollment tokens (OTT):**
- CSPRNG-generated, 256-bit entropy
- Single-use: consumed on first successful claim
- Time-limited: 10-minute default (configurable)
- Optionally IP-bound: `clawden fleet enroll coder-1 --bind-ip 1.2.3.4`
- Revocable before claim: `clawden fleet enroll cancel coder-1`

#### Sealed Envelope Encryption

Secrets travel from control plane to agent as **sealed envelopes** — encrypted to the agent's node public key:

```
Seal (control plane):
  plaintext_secrets = { "openai-key": "sk-...", "github-pat": "ghp_..." }
  sealed = crypto_box_seal(plaintext_secrets, agent_node_pub_key)
  → Only the agent's node.key can open this

Unseal (agent side):
  plaintext = crypto_box_seal_open(sealed, node_key)
  → Write to tmpfs: ~/.clawden/agent-vault/secrets/
  → Export as env vars to runtime process
```

- Uses NaCl `crypto_box_seal` (X25519 + XSalsa20-Poly1305) or age encryption
- Double-layered: sealed envelope inside TLS transport — even if TLS is MITM'd, secrets are still encrypted
- Re-seal on rotation: control plane encrypts new secrets with same node public key, pushes over control channel

#### Control Channel

After enrollment, a persistent control channel connects each agent node to the control plane:

| Aspect | Design |
|---|---|
| **Transport** | HTTPS + WebSocket (agent connects outbound — works behind NAT/firewalls) |
| **Auth** | mTLS using node certificate (issued at enrollment) + JWT bearer for per-request auth |
| **Heartbeat** | Agent → control plane every 60s (configurable). Carries: agent status, resource usage, audit event batch |
| **Secret push** | Control plane → agent: re-sealed envelope when secrets rotate |
| **Token refresh** | Control plane → agent: fresh JWT before current one expires |
| **Revocation** | Control plane → agent: immediate disconnect + secret wipe command |
| **Message relay** | Control plane relays inter-agent messages between nodes (agents don't connect directly to each other) |
| **Reconnection** | Exponential backoff (1s → 2s → 4s → ... → 5min cap). Cached credentials valid for grace period. |

**Why WebSocket outbound (not inbound)?** Remote agents are often behind NAT, firewalls, or cloud networking that blocks inbound connections. The agent initiates the WebSocket connection to the control plane (like an employee VPN connecting out to the corporate network). The control plane never needs to connect inbound to an agent.

#### Grace Period on Partition

When a remote agent loses connectivity to the control plane:

1. **0 – grace_period** (default: 1 hour): Agent continues operating with cached credentials and last-known scopes. Audit events are queued locally.
2. **grace_period – 2x grace_period**: Agent enters "degraded" state. Can fulfill in-progress tasks but won't accept new ones. Dashboard shows warning.
3. **> 2x grace_period**: Agent self-suspends. Runtime process is stopped. Secrets remain in tmpfs until host reboot or explicit wipe.

The human can configure this per-role:

```yaml
fleet:
  auth:
    partition_policy:
      leader:
        grace_period: 2h    # leaders get more time — they may be coordinating
      worker:
        grace_period: 30m   # workers should reconnect quickly
      default:
        grace_period: 1h
        on_expire: suspend   # suspend | shutdown | continue-readonly
```

#### Fleet Configuration with Nodes

```yaml
# clawden.yaml — distributed fleet
fleet:
  control_plane:
    listen: 0.0.0.0:7443               # Fleet API endpoint
    tls:
      cert: /etc/clawden/fleet.crt
      key: /etc/clawden/fleet.key
    auth:
      vault_backend: os-keychain
      enrollment_ttl: 10m
      token_lifetime: 1h
      partition_policy:
        default:
          grace_period: 1h
          on_expire: suspend

  nodes:
    codespace-dev:
      host: "codespace://user/repo"     # informational
      agents: [coordinator]
    vps-amsterdam:
      host: "vps://1.2.3.4"
      agents: [coder-1, coder-2]
    cloud-gpu:
      host: "gcp://project/instance"
      agents: [researcher]

agents:
  coordinator:
    runtime: openclaw
    role: leader
    node: codespace-dev
    scopes:
      - channel-access:telegram
      - message-send:*
      - task-delegate:*
    secrets:
      anthropic-key: $COORDINATOR_ANTHROPIC_KEY
      telegram-token: $TELEGRAM_BOT_TOKEN

  coder-1:
    runtime: zeroclaw
    role: worker
    node: vps-amsterdam
    scopes:
      - tool-use:git,code,test
      - network-access:github.com,npmjs.com
    secrets:
      openai-key: $CODER_OPENAI_KEY
      github-pat: $CODER_GITHUB_TOKEN

  researcher:
    runtime: picoclaw
    role: specialist
    node: cloud-gpu
    scopes:
      - tool-use:web-search,summarize
      - network-access:*
    secrets:
      anthropic-key: $RESEARCHER_ANTHROPIC_KEY
      web-search-key: $TAVILY_API_KEY
```

#### Local-First, Distributed-Ready

The distributed architecture is **additive** — single-host fleets work exactly as before, with zero configuration overhead:

| Deployment | What changes |
|---|---|
| **Single host** (`clawden up`) | All agents are local child processes. Secrets injected via env vars. Message bus uses in-process tokio channels. No enrollment needed. No mTLS. No control channel. |
| **Mixed** | Some agents local, some remote. Local agents get env var injection. Remote agents go through enrollment + sealed envelope. Message bus routes local messages in-process, remote messages via control channel relay. |
| **Fully distributed** | All agents remote. Control plane is a lightweight server that manages enrollment, relays messages, aggregates audit. Can run on a $5/mo VPS or in a container. |

This means the simplest path (`clawden up` on one machine) has zero auth ceremony — the distributed machinery activates only when `fleet.nodes` or `clawden fleet enroll` is used.

## Plan

- [ ] **Phase 1: Identity model** — Define `Principal`, `AgentRole`, `Scope` types in `clawden-core`. Add agent identity to fleet registration.
- [ ] **Phase 2: Scoped vault (local)** — Implement per-agent secret directories on the control plane host. Migrate from flat `SecretVault` to per-principal vault with `fleet.key` encryption.
- [ ] **Phase 3: Local process isolation** — Update process supervisor (spec 054) to inject only agent-scoped env vars for local agents. Verify no cross-agent secret leakage.
- [ ] **Phase 4: Identity tokens** — JWT issuance and verification for agents. Message bus authenticates senders. Token rotation on configurable interval. Node-binding for remote agents.
- [ ] **Phase 5: Enrollment protocol** — Implement `clawden fleet enroll <agent>` (control plane) and `clawden agent join <fleet-url> --token <OTT>` (remote host). One-time token generation, node keypair creation, sealed envelope exchange.
- [ ] **Phase 6: Remote secret delivery** — Sealed envelope encryption (age/NaCl box). Secret push on rotation. tmpfs storage on remote hosts. Re-seal on key rotation.
- [ ] **Phase 7: Control channel** — mTLS between control plane and remote agent nodes. Heartbeat, token refresh, secret rotation, revocation propagation over encrypted channel.
- [ ] **Phase 8: Config schema** — Add `fleet.auth`, `fleet.nodes`, `agents.*.scopes`, `agents.*.secrets`, `agents.*.node` to `clawden.yaml` schema. Role-based default scopes.
- [ ] **Phase 9: Human auth** — `clawden auth login/logout/token` commands. Dashboard bearer auth. Session management.
- [ ] **Phase 10: Audit trail** — Auth events in persistent store. Distributed audit aggregation from remote nodes. `clawden audit` CLI for viewing. Scope violation alerts.
- [ ] **Phase 11: Dashboard integration** — Auth-gated dashboard access. Fleet topology view (which agents on which hosts). Per-agent secret management UI (create/rotate/revoke). Scope violation feed.

## Test

### Local fleet
- [ ] Agent process receives only its own secrets as env vars — not other agents' or human's
- [ ] Agent with `tool-use:git` scope cannot invoke `web-search` tool
- [ ] Message from agent A to agent B is rejected if A lacks `message-send:B` scope
- [ ] Identity token expires and is auto-rotated by supervisor without agent downtime
- [ ] `clawden auth token create` produces a working bearer token for API access
- [ ] Scope violation is logged and visible in `clawden audit`
- [ ] Decommissioned agent's vault is purged and identity token revoked
- [ ] `fleet.key` rotation re-encrypts all vault entries without data loss
- [ ] Multi-human deployment: user A cannot revoke user B's tokens without admin scope

### Distributed fleet
- [ ] `clawden fleet enroll coder-1` generates a one-time token that expires in 10 minutes
- [ ] `clawden agent join` with valid OTT completes enrollment and receives sealed secrets
- [ ] `clawden agent join` with expired or reused OTT is rejected
- [ ] Remote agent's secrets are stored in tmpfs and cleared on reboot
- [ ] Sealed envelope cannot be decrypted without the agent's node key
- [ ] Secret rotation pushes re-sealed envelope to remote agent without downtime
- [ ] Agent on host A cannot use identity token stolen and replayed from host B (node_id mismatch)
- [ ] Network partition: agent continues operating with cached credentials for grace period, then self-suspends
- [ ] `clawden fleet revoke coder-1` propagates revocation to remote host within heartbeat interval
- [ ] Control channel rejects connections without valid mTLS certificate after enrollment
- [ ] Audit events from remote agents are aggregated to the control plane's persistent store
- [ ] Fleet with agents on 3+ different hosts: all agents enroll, receive scoped secrets, and communicate via message bus

## Notes

### Alternatives Considered

- **HashiCorp Vault integration**: Too heavy for personal/small-team use. Our vault is file-based with OS keychain encryption. Can add Vault backend later via `vault_backend: hashicorp` config.
- **WireGuard mesh between all agents**: Simpler than mTLS but requires kernel-level or userspace WireGuard on every host. Some cloud environments (Codespaces, serverless) don't support it. mTLS works everywhere HTTPS works.
- **RBAC with custom policy engine (OPA/Rego)**: Over-engineered for v1. Static role → scope mapping covers 90% of use cases. Can add policy engine as a Scope variant later.
- **Per-agent OS users**: Strong isolation but impractical for Docker containers and adds operational burden. Process-level env var scoping is sufficient for v1.
- **Centralized secret server (always-online)**: Requires the control plane to be reachable whenever an agent starts. Sealed envelope delivery means agents can start offline with cached secrets — more resilient to control plane downtime.
- **SSH tunnels for control channel**: Works but SSH session management is fragile for long-lived connections. mTLS over HTTPS is more robust and reuses the same stack as the dashboard API.

### Design Decisions

- **JWT over API keys for agent identity**: API keys are static; JWTs expire and carry embedded scopes. This prevents token reuse after decommission.
- **File-based vault on control plane, sealed envelopes for remote**: Control plane keeps the source of truth. Remote agents receive encrypted copies. No shared filesystem assumption.
- **Scopes are additive**: An agent starts with its role defaults and can have additional scopes granted. There is no "deny" — you simply don't grant the scope.
- **Enrollment over invitation, not discovery**: Agents don't find the fleet — the fleet invites them. This prevents rogue agents from joining by scanning.
- **mTLS for control channel, not for message bus**: mTLS secures the transport between hosts. The message bus uses JWT auth on top of the encrypted channel — defense in depth without doubling certificate management.
- **Graceful degradation on network partition**: Remote agents keep working with cached credentials for a configurable grace period rather than hard-failing. The human decides what happens after grace expires (suspend, continue read-only, or shutdown).

### Open Questions

- Should agents be able to request scope escalation at runtime? (e.g., worker needs web access for a specific task → asks leader → leader asks human → temporary scope grant)
- How does this interact with tool-level auth? Some MCP tools carry their own credentials — should ClawDen manage those too or leave them to the tool?
- Should there be a "sandbox" role with no network access and no persistent memory — for running untrusted code?
- Cross-fleet auth: if two fleets need to collaborate, how do agents from fleet A authenticate to fleet B? Possible approach: fleet-level identity tokens signed by each fleet's key, with a trust relationship established by exchanging fleet.pub keys.
- Should the control plane support a "relay" mode where it proxies messages between agents that can't reach each other directly? (e.g., agent on a corporate LAN + agent on a public cloud)
- How to handle agents behind NAT? Control plane could use WebSocket (agent connects out to control plane) instead of requiring inbound connections.