use anyhow::Result;
use async_trait::async_trait;
use clawden_core::{
    AgentConfig, AgentHandle, AgentMessage, AgentMetrics, AgentResponse, ClawAdapter, ClawRuntime,
    EventStream, HealthStatus, InstallConfig, RuntimeConfig, RuntimeMetadata, Skill, SkillManifest,
};

pub struct NanoClawAdapter;

#[async_trait]
impl ClawAdapter for NanoClawAdapter {
    fn metadata(&self) -> RuntimeMetadata {
        RuntimeMetadata {
            runtime: ClawRuntime::NanoClaw,
            version: "unknown".to_string(),
            language: "typescript".to_string(),
            capabilities: vec!["chat".to_string(), "skills".to_string()],
        }
    }

    async fn install(&self, _config: &InstallConfig) -> Result<()> {
        Ok(())
    }

    async fn start(&self, config: &AgentConfig) -> Result<AgentHandle> {
        Ok(AgentHandle {
            id: format!("nanoclaw-{}", config.name),
            name: config.name.clone(),
            runtime: ClawRuntime::NanoClaw,
        })
    }

    async fn stop(&self, _handle: &AgentHandle) -> Result<()> {
        Ok(())
    }

    async fn restart(&self, _handle: &AgentHandle) -> Result<()> {
        Ok(())
    }

    async fn health(&self, _handle: &AgentHandle) -> Result<HealthStatus> {
        Ok(HealthStatus::Unknown)
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

    async fn get_config(&self, _handle: &AgentHandle) -> Result<RuntimeConfig> {
        Ok(RuntimeConfig {
            values: serde_json::json!({ "runtime": "nanoclaw" }),
        })
    }

    async fn set_config(&self, _handle: &AgentHandle, _config: &RuntimeConfig) -> Result<()> {
        Ok(())
    }

    async fn list_skills(&self, _handle: &AgentHandle) -> Result<Vec<Skill>> {
        Ok(vec![])
    }

    async fn install_skill(&self, _handle: &AgentHandle, _skill: &SkillManifest) -> Result<()> {
        Ok(())
    }
}
