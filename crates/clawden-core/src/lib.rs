use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClawRuntime {
    OpenClaw,
    ZeroClaw,
    PicoClaw,
    NanoClaw,
    IronClaw,
    NullClaw,
    MicroClaw,
    MimiClaw,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeMetadata {
    pub runtime: ClawRuntime,
    pub version: String,
    pub language: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallConfig {
    pub runtime: ClawRuntime,
    pub image: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub runtime: ClawRuntime,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHandle {
    pub id: String,
    pub name: String,
    pub runtime: ClawRuntime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetrics {
    pub cpu_percent: f32,
    pub memory_mb: f32,
    pub queue_depth: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub values: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub version: String,
    pub runtimes: Vec<ClawRuntime>,
}

pub type EventStream = Vec<serde_json::Value>;

#[async_trait]
pub trait ClawAdapter: Send + Sync {
    fn metadata(&self) -> RuntimeMetadata;

    async fn install(&self, config: &InstallConfig) -> Result<()>;
    async fn start(&self, config: &AgentConfig) -> Result<AgentHandle>;
    async fn stop(&self, handle: &AgentHandle) -> Result<()>;
    async fn restart(&self, handle: &AgentHandle) -> Result<()>;

    async fn health(&self, handle: &AgentHandle) -> Result<HealthStatus>;
    async fn metrics(&self, handle: &AgentHandle) -> Result<AgentMetrics>;

    async fn send(&self, handle: &AgentHandle, message: &AgentMessage) -> Result<AgentResponse>;
    async fn subscribe(&self, handle: &AgentHandle, event: &str) -> Result<EventStream>;

    async fn get_config(&self, handle: &AgentHandle) -> Result<RuntimeConfig>;
    async fn set_config(&self, handle: &AgentHandle, config: &RuntimeConfig) -> Result<()>;

    async fn list_skills(&self, handle: &AgentHandle) -> Result<Vec<Skill>>;
    async fn install_skill(&self, handle: &AgentHandle, skill: &SkillManifest) -> Result<()>;
}
