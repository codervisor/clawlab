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

pub struct NanoClawAdapter;

fn config_store() -> &'static Mutex<HashMap<String, RuntimeConfig>> {
    static STORE: OnceLock<Mutex<HashMap<String, RuntimeConfig>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[async_trait]
impl ClawAdapter for NanoClawAdapter {
    fn metadata(&self) -> RuntimeMetadata {
        let mut channel_support = HashMap::new();
        channel_support.insert(ChannelType::Telegram, ChannelSupport::Via("skill".into()));
        channel_support.insert(ChannelType::Discord, ChannelSupport::Via("skill".into()));
        channel_support.insert(ChannelType::Slack, ChannelSupport::Via("skill".into()));
        channel_support.insert(ChannelType::Whatsapp, ChannelSupport::Native);

        RuntimeMetadata {
            runtime: ClawRuntime::NanoClaw,
            version: "unknown".to_string(),
            language: "typescript".to_string(),
            capabilities: vec!["chat".to_string(), "skills".to_string()],
            default_port: None,
            config_format: Some("code".to_string()),
            channel_support,
        }
    }

    async fn install(&self, _config: &InstallConfig) -> Result<()> {
        Ok(())
    }

    async fn start(&self, config: &AgentConfig) -> Result<AgentHandle> {
        let container_id = start_container(ClawRuntime::NanoClaw, config)?;
        let handle = AgentHandle {
            id: container_id,
            name: config.name.clone(),
            runtime: ClawRuntime::NanoClaw,
        };

        set_stored_config(
            config_store(),
            &handle.id,
            runtime_config_values("nanoclaw", config),
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
            content: format!("NanoClaw echo: {}", message.content),
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
            values: serde_json::json!({ "runtime": "nanoclaw" }),
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
