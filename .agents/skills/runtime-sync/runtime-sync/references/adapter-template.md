# Canonical Adapter Template

Every runtime adapter MUST follow this exact pattern. Deviations cause inconsistencies
that break orchestration assumptions.

## Table of Contents

- [Imports](#imports)
- [Struct & Config Store](#struct--config-store)
- [ClawAdapter Implementation](#clawadapter-implementation)
- [Tests](#tests)
- [Known Variations](#known-variations)

## Imports

```rust
use crate::docker_runtime::{
    container_running, get_stored_config, remove_stored_config, restart_container,
    runtime_config_values, set_stored_config, start_container, stop_container,
};
use anyhow::Result;
use async_trait::async_trait;
use clawden_core::{
    AgentConfig, AgentHandle, AgentMessage, AgentMetrics, AgentResponse, ChannelSupport,
    ChannelType, ClawAdapter, ClawRuntime, EventStream, HealthStatus, InstallConfig, RuntimeConfig,
    RuntimeMetadata, Skill, SkillManifest,
};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
```

**Rules:**
- Do NOT import `bail` from anyhow — stub methods return `Ok(...)`, never error
- Import exactly the types listed above — no more, no fewer

## Struct & Config Store

```rust
pub struct {Name}Adapter;

fn config_store() -> &'static Mutex<HashMap<String, RuntimeConfig>> {
    static STORE: OnceLock<Mutex<HashMap<String, RuntimeConfig>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}
```

Replace `{Name}` with PascalCase runtime name (e.g., `OpenFangAdapter`).

## ClawAdapter Implementation

### metadata()

```rust
fn metadata(&self) -> RuntimeMetadata {
    let mut channel_support = HashMap::new();
    // Insert channel support entries (see channel-matrix reference)

    RuntimeMetadata {
        runtime: ClawRuntime::{Variant},
        version: "unknown".to_string(),
        language: "{language}".to_string(),            // "rust", "typescript", "go", "zig", etc.
        capabilities: vec![/* capability strings */],
        default_port: Some({port}),                     // or None
        config_format: Some("{format}".to_string()),    // "toml", "json5", "json", "code", etc.
        channel_support,
    }
}
```

### Lifecycle methods (MUST be identical pattern)

```rust
async fn install(&self, _config: &InstallConfig) -> Result<()> {
    Ok(())
}

async fn start(&self, config: &AgentConfig) -> Result<AgentHandle> {
    let container_id = start_container(ClawRuntime::{Variant}, config)?;
    let handle = AgentHandle {
        id: container_id,
        name: config.name.clone(),
        runtime: ClawRuntime::{Variant},
    };
    set_stored_config(
        config_store(),
        &handle.id,
        runtime_config_values("{slug}", config),
    );
    Ok(handle)
}

async fn stop(&self, handle: &AgentHandle) -> Result<()> {
    stop_container(&handle.id)?;
    remove_stored_config(config_store(), &handle.id);
    Ok(())
}

async fn restart(&self, handle: &AgentHandle) -> Result<()> {
    restart_container(&handle.id)?;
    Ok(())
}
```

### Health & Metrics (MUST be identical)

```rust
async fn health(&self, handle: &AgentHandle) -> Result<HealthStatus> {
    if container_running(&handle.id)? {
        Ok(HealthStatus::Healthy)
    } else {
        Ok(HealthStatus::Unhealthy)
    }
}

async fn metrics(&self, _handle: &AgentHandle) -> Result<AgentMetrics> {
    Ok(AgentMetrics {
        cpu_percent: 0.0,
        memory_mb: 0.0,
        queue_depth: 0,
    })
}
```

### Communication (MUST follow echo pattern)

```rust
async fn send(&self, _handle: &AgentHandle, message: &AgentMessage) -> Result<AgentResponse> {
    Ok(AgentResponse {
        content: format!("{Name} echo: {}", message.content),
    })
}

async fn subscribe(&self, _handle: &AgentHandle, _event: &str) -> Result<EventStream> {
    Ok(vec![])
}
```

**CRITICAL:** Never use `bail!()` in `send()`. All adapters use the echo stub pattern.

### Config (MUST include runtime key in fallback)

```rust
async fn get_config(&self, handle: &AgentHandle) -> Result<RuntimeConfig> {
    if let Some(config) = get_stored_config(config_store(), &handle.id) {
        return Ok(config);
    }
    Ok(RuntimeConfig {
        values: serde_json::json!({ "runtime": "{slug}" }),
    })
}

async fn set_config(&self, handle: &AgentHandle, config: &RuntimeConfig) -> Result<()> {
    set_stored_config(config_store(), &handle.id, config.clone());
    Ok(())
}
```

**CRITICAL:** Fallback JSON MUST include `"runtime": "{slug}"`. Never return empty `{}`.

### Skills (MUST be identical)

```rust
async fn list_skills(&self, _handle: &AgentHandle) -> Result<Vec<Skill>> {
    Ok(vec![])
}

async fn install_skill(&self, _handle: &AgentHandle, _skill: &SkillManifest) -> Result<()> {
    Ok(())
}
```

## Tests

Every adapter MUST include a config persistence test:

```rust
#[cfg(test)]
mod tests {
    use super::{Name}Adapter;
    use clawden_core::{AgentConfig, ClawAdapter, ClawRuntime};

    #[test]
    fn start_persists_forwarded_runtime_config() {
        std::env::set_var("CLAWDEN_ADAPTER_DRY_RUN", "1");
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime should initialize");
        runtime.block_on(async {
            let adapter = {Name}Adapter;
            let handle = adapter
                .start(&AgentConfig {
                    name: "test-agent".to_string(),
                    runtime: ClawRuntime::{Variant},
                    model: None,
                    env_vars: vec![("OPENAI_API_KEY".to_string(), "sk-test".to_string())],
                    channels: vec!["telegram".to_string()],
                    tools: vec!["git".to_string(), "http".to_string()],
                })
                .await
                .expect("adapter start should succeed");

            let cfg = adapter
                .get_config(&handle)
                .await
                .expect("adapter config should be readable");
            assert_eq!(cfg.values["channels"][0].as_str(), Some("telegram"));
            assert_eq!(cfg.values["tools"][0].as_str(), Some("git"));
            assert_eq!(cfg.values["env_vars"][0][0].as_str(), Some("OPENAI_API_KEY"));
        });
        std::env::remove_var("CLAWDEN_ADAPTER_DRY_RUN");
    }
}
```

## Known Variations

Only the following fields vary per runtime — everything else is identical:

| Field | Example values |
|-------|---------------|
| `ClawRuntime::` variant | `OpenClaw`, `ZeroClaw`, `PicoClaw`, etc. |
| `language` | `"rust"`, `"typescript"`, `"go"`, `"zig"` |
| `capabilities` | `["chat", "reasoning"]`, `["chat", "tools"]`, `["chat", "skills"]` |
| `default_port` | `Some(42617)`, `Some(18789)`, `None` |
| `config_format` | `"toml"`, `"json5"`, `"json"`, `"code"` |
| `channel_support` map | Varies per runtime (see channel matrix) |
| `runtime_config_values` slug | `"zeroclaw"`, `"openclaw"`, etc. |
