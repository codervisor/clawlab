---
status: planned
created: 2026-02-26
priority: critical
tags:
- core
- adapter
- cri
parent: 009-orchestration-platform
created_at: 2026-02-26T02:08:29.575446436Z
updated_at: 2026-02-26T02:08:40.054769542Z
---

# Claw Runtime Interface (CRI) / Adapter Layer

## Overview

The Claw Runtime Interface (CRI) is the adapter layer that abstracts communication with heterogeneous claw runtimes. Like Kubernetes' Container Runtime Interface, CRI provides a unified Rust trait that each claw runtime implements via a driver/adapter.

## Design

### Core Trait

```rust
#[async_trait]
pub trait ClawAdapter: Send + Sync {
    fn runtime(&self) -> &ClawRuntime; // metadata: name, version, lang, capabilities

    // Lifecycle
    async fn install(&self, config: &InstallConfig) -> Result<()>;
    async fn start(&self, config: &AgentConfig) -> Result<AgentHandle>;
    async fn stop(&self, handle: &AgentHandle) -> Result<()>;
    async fn restart(&self, handle: &AgentHandle) -> Result<()>;

    // Health
    async fn health(&self, handle: &AgentHandle) -> Result<HealthStatus>;
    async fn metrics(&self, handle: &AgentHandle) -> Result<AgentMetrics>;

    // Communication
    async fn send(&self, handle: &AgentHandle, message: &AgentMessage) -> Result<AgentResponse>;
    async fn subscribe(&self, handle: &AgentHandle, event: &str) -> Result<EventStream>;

    // Configuration
    async fn get_config(&self, handle: &AgentHandle) -> Result<RuntimeConfig>;
    async fn set_config(&self, handle: &AgentHandle, config: &RuntimeConfig) -> Result<()>;

    // Skills
    async fn list_skills(&self, handle: &AgentHandle) -> Result<Vec<Skill>>;
    async fn install_skill(&self, handle: &AgentHandle, skill: &SkillManifest) -> Result<()>;
}
```

### Supported Runtimes (Initial)

| Runtime | Language | Stars | Communication Method | Adapter Strategy |
|---------|----------|-------|---------------------|------------------|
| OpenClaw | TypeScript (Node.js) | 229K+ | WS Gateway (port 18789) | WebSocket client |
| ZeroClaw | Rust | 19K+ | HTTP Gateway (port 42617) + CLI | REST + subprocess |
| PicoClaw | Go | 20K+ | HTTP Gateway + CLI | REST + subprocess |
| NanoClaw | TypeScript (Node.js) | 15K+ | Agent SDK + Docker isolation | REST + subprocess |
| IronClaw | Rust | 3.5K+ | HTTP webhooks + REPL + WASM | REST + subprocess |
| NullClaw | Zig | 2.2K+ | HTTP Gateway (port 3000) + CLI | REST + subprocess |
| MicroClaw | Rust | 410+ | Multi-channel + Web UI | REST + subprocess |
| MimiClaw | C (ESP32-S3) | 3.3K+ | Telegram + WebSocket (port 18789) | Serial/MQTT bridge |

### Adapter Registration
Adapters are registered via Rust feature flags (compile-time) or dynamic loading from `~/.clawlab/adapters/` (shared libraries). Built-in adapters are compiled into the binary by default.

## Plan

- [ ] Define `ClawAdapter` Rust trait and core types in `crates/clawlab-core`
- [ ] Implement `OpenClawAdapter` (HTTP REST client, most mature ecosystem)
- [ ] Implement `ZeroClawAdapter` (native Rust, most natural integration)
- [ ] Implement `PicoClawAdapter` (HTTP + subprocess for Go binary)
- [ ] Implement `NanoClawAdapter` (HTTP + subprocess for Node.js)
- [ ] Create adapter registry with feature-flag and dynamic loading
- [ ] Add adapter discovery and auto-detection

## Test

- [ ] Each adapter can connect to its respective runtime
- [ ] Adapters correctly report health status
- [ ] Plugin loader discovers and registers adapters
- [ ] Adapters handle connection failures gracefully
