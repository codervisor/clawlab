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

pub struct ZeroClawAdapter;

fn config_store() -> &'static Mutex<HashMap<String, RuntimeConfig>> {
    static STORE: OnceLock<Mutex<HashMap<String, RuntimeConfig>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[async_trait]
impl ClawAdapter for ZeroClawAdapter {
    fn metadata(&self) -> RuntimeMetadata {
        let mut channel_support = HashMap::new();
        channel_support.insert(ChannelType::Telegram, ChannelSupport::Native);
        channel_support.insert(ChannelType::Discord, ChannelSupport::Native);
        channel_support.insert(ChannelType::Slack, ChannelSupport::Native);
        channel_support.insert(
            ChannelType::Whatsapp,
            ChannelSupport::Via("Meta Cloud API".into()),
        );
        channel_support.insert(ChannelType::Signal, ChannelSupport::Native);
        channel_support.insert(ChannelType::Feishu, ChannelSupport::Native);
        channel_support.insert(ChannelType::Matrix, ChannelSupport::Native);
        channel_support.insert(ChannelType::Email, ChannelSupport::Native);
        channel_support.insert(ChannelType::Mattermost, ChannelSupport::Native);
        channel_support.insert(ChannelType::Irc, ChannelSupport::Native);
        channel_support.insert(ChannelType::Imessage, ChannelSupport::Native);
        channel_support.insert(ChannelType::Nostr, ChannelSupport::Native);

        RuntimeMetadata {
            runtime: ClawRuntime::ZeroClaw,
            version: "unknown".to_string(),
            language: "rust".to_string(),
            capabilities: vec!["chat".to_string(), "reasoning".to_string()],
            default_port: Some(42617),
            config_format: Some("toml".to_string()),
            channel_support,
        }
    }

    async fn install(&self, _config: &InstallConfig) -> Result<()> {
        Ok(())
    }

    async fn start(&self, config: &AgentConfig) -> Result<AgentHandle> {
        let container_id = start_container(ClawRuntime::ZeroClaw, config)?;
        let handle = AgentHandle {
            id: container_id,
            name: config.name.clone(),
            runtime: ClawRuntime::ZeroClaw,
        };

        set_stored_config(
            config_store(),
            &handle.id,
            runtime_config_values("zeroclaw", config),
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

    async fn send(&self, _handle: &AgentHandle, message: &AgentMessage) -> Result<AgentResponse> {
        Ok(AgentResponse {
            content: format!("ZeroClaw echo: {}", message.content),
        })
    }

    async fn subscribe(&self, _handle: &AgentHandle, _event: &str) -> Result<EventStream> {
        Ok(vec![])
    }

    async fn get_config(&self, handle: &AgentHandle) -> Result<RuntimeConfig> {
        if let Some(config) = get_stored_config(config_store(), &handle.id) {
            return Ok(config);
        }
        Ok(RuntimeConfig {
            values: serde_json::json!({ "runtime": "zeroclaw" }),
        })
    }

    async fn set_config(&self, handle: &AgentHandle, config: &RuntimeConfig) -> Result<()> {
        set_stored_config(config_store(), &handle.id, config.clone());
        Ok(())
    }

    async fn list_skills(&self, _handle: &AgentHandle) -> Result<Vec<Skill>> {
        Ok(vec![])
    }

    async fn install_skill(&self, _handle: &AgentHandle, _skill: &SkillManifest) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ZeroClawAdapter;
    use clawden_core::{AgentConfig, ClawAdapter, ClawRuntime};

    #[test]
    fn start_persists_forwarded_runtime_config() {
        std::env::set_var("CLAWDEN_ADAPTER_DRY_RUN", "1");
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime should initialize");
        runtime.block_on(async {
            let adapter = ZeroClawAdapter;
            let handle = adapter
                .start(&AgentConfig {
                    name: "test-agent".to_string(),
                    runtime: ClawRuntime::ZeroClaw,
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
            assert_eq!(
                cfg.values["channels"][0].as_str(),
                Some("telegram"),
                "channel passthrough should be retained"
            );
            assert_eq!(
                cfg.values["tools"][0].as_str(),
                Some("git"),
                "tools passthrough should be retained"
            );
            assert_eq!(
                cfg.values["env_vars"][0][0].as_str(),
                Some("OPENAI_API_KEY"),
                "env var passthrough should be retained"
            );
        });
        std::env::remove_var("CLAWDEN_ADAPTER_DRY_RUN");
    }
}
