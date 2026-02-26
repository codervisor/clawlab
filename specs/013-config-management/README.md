---
status: planned
created: 2026-02-26
priority: medium
tags:
- core
- config
- secrets
depends_on:
- 010-claw-runtime-interface
created_at: 2026-02-26T02:08:29.575930222Z
updated_at: 2026-02-26T02:08:40.055694095Z
---

# Unified Configuration Management

## Overview

Each claw runtime has its own config format (JSON, TOML, env vars, markdown). ClawLab provides a unified configuration layer that translates between a canonical schema and runtime-specific formats.

## Design

### Canonical Config Schema
```rust
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct ClawLabConfig {
    pub agent: AgentConfig,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct AgentConfig {
    pub name: String,
    pub runtime: String,
    pub model: ModelConfig,          // provider, model name, API key ref
    pub tools: Vec<ToolConfig>,      // enabled tools and permissions
    pub channels: Vec<ChannelConfig>,// messaging channels
    pub memory: MemoryConfig,        // memory backend settings
    pub security: SecurityConfig,    // sandbox, allowlists
    pub schedule: Option<ScheduleConfig>, // cron jobs, heartbeat
    #[serde(flatten)]
    pub extras: HashMap<String, Value>, // runtime-specific extensions
}
```

Config files use **TOML** as canonical format (aligns with Rust/Cargo ecosystem, human-readable).

### Config Translation
Each CRI adapter includes a config translator trait:
- `to_runtime_config(&canonical)` → runtime-specific format (JSON/TOML/env/markdown)
- `from_runtime_config(&native)` → canonical format
- Validates required fields per runtime via serde + custom validators
- Handles runtime-specific extensions via `extras` field

### Secret Management
- API keys stored in encrypted vault (age/sops or system keychain)
- Referenced by name in config, injected at deploy time
- Never stored in plain text or committed to git

## Plan

- [ ] Define canonical config schema with serde + validation
- [ ] Implement config translator trait in CRI
- [ ] Build OpenClaw config translator (JSON ↔ canonical TOML)
- [ ] Build ZeroClaw config translator (TOML ↔ canonical TOML)
- [ ] Build PicoClaw config translator (JSON ↔ canonical TOML)
- [ ] Implement encrypted secret vault (age encryption or system keychain)
- [ ] Add config diff and drift detection

## Test

- [ ] Canonical config round-trips through each translator
- [ ] Invalid configs are rejected with clear error messages
- [ ] Secrets are never exposed in logs or API responses
- [ ] Config drift detection identifies out-of-sync agents
