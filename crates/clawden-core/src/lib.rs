mod audit;
mod channel_registry;
mod channels;
mod discovery;
mod install;
mod lifecycle;
mod manager;
mod process;
mod provider_registry;
mod runtime_descriptor;
mod swarm;
mod util;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use audit::{append_audit, AuditEvent, AuditLog};
pub use channel_registry::{
    channel_descriptor, channel_descriptors, channel_token_env_name, known_channel_env_vars,
    ChannelDescriptor, CHANNELS,
};
pub use channels::{
    BindChannelRequest, BindingConflict, ChannelConfigRequest, ChannelCredentialCheck,
    ChannelHealthEntry, ChannelStore, ChannelTypeSummary, MatrixRow,
};
pub use discovery::{DiscoveredEndpoint, DiscoveryMethod, DiscoveryService};
pub use install::{
    runtime_default_start_args, runtime_subcommand_hints, runtime_supports_config_dir,
    version_satisfies, InstallOutcome, InstalledRuntime, RuntimeInstaller, VersionCheck,
};
pub use lifecycle::AgentState;
pub use manager::{AgentRecord, LifecycleManager, ManagerError};
pub use process::{
    ExecutionMode, LogLine, LogStream, ProcessInfo, ProcessManager, RuntimeProcessStatus,
    StopOutcome,
};
pub use provider_registry::{
    infer_provider_from_host_env, known_provider_env_vars, provider_descriptor,
    provider_descriptors, provider_env_candidates, provider_env_vars, provider_primary_env_var,
    ProviderDescriptor, PROVIDERS,
};
pub use runtime_descriptor::{
    direct_install_descriptors, runtime_descriptor, runtime_descriptor_for, runtime_descriptors,
    ConfigDirFlag, ConfigFormat, InstallSource, RuntimeDescriptor, VersionSource,
};
pub use swarm::{SwarmCoordinator, SwarmMember, SwarmRole};
pub use util::{current_unix_ms, runtime_env_prefix};

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
    OpenFang,
}

// Display, from_str_loose, and as_slug are implemented in
// runtime_descriptor.rs — driven by descriptor data so adding a new
// runtime only requires one DESCRIPTORS entry.

// ---------------------------------------------------------------------------
// Channel types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Telegram,
    Discord,
    Slack,
    Whatsapp,
    Signal,
    Matrix,
    Email,
    Feishu,
    Dingtalk,
    Mattermost,
    Irc,
    Teams,
    Imessage,
    GoogleChat,
    Qq,
    Line,
    Nostr,
}

impl std::fmt::Display for ChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ChannelType::Telegram => "telegram",
            ChannelType::Discord => "discord",
            ChannelType::Slack => "slack",
            ChannelType::Whatsapp => "whatsapp",
            ChannelType::Signal => "signal",
            ChannelType::Matrix => "matrix",
            ChannelType::Email => "email",
            ChannelType::Feishu => "feishu",
            ChannelType::Dingtalk => "dingtalk",
            ChannelType::Mattermost => "mattermost",
            ChannelType::Irc => "irc",
            ChannelType::Teams => "teams",
            ChannelType::Imessage => "imessage",
            ChannelType::GoogleChat => "google_chat",
            ChannelType::Qq => "qq",
            ChannelType::Line => "line",
            ChannelType::Nostr => "nostr",
        };
        write!(f, "{s}")
    }
}

impl ChannelType {
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "telegram" => Some(Self::Telegram),
            "discord" => Some(Self::Discord),
            "slack" => Some(Self::Slack),
            "whatsapp" => Some(Self::Whatsapp),
            "signal" => Some(Self::Signal),
            "matrix" => Some(Self::Matrix),
            "email" => Some(Self::Email),
            "feishu" | "lark" => Some(Self::Feishu),
            "dingtalk" => Some(Self::Dingtalk),
            "mattermost" => Some(Self::Mattermost),
            "irc" => Some(Self::Irc),
            "teams" => Some(Self::Teams),
            "imessage" => Some(Self::Imessage),
            "google_chat" | "googlechat" => Some(Self::GoogleChat),
            "qq" => Some(Self::Qq),
            "line" => Some(Self::Line),
            "nostr" => Some(Self::Nostr),
            _ => None,
        }
    }
}

/// Describes how a runtime natively supports a channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelSupport {
    /// Runtime has built-in native support.
    Native,
    /// Supported via a runtime-specific mechanism (e.g. skill, WASM plugin).
    Via(String),
    /// Not natively supported — requires ClawDen channel proxy.
    Unsupported,
}

/// Per-channel instance credential/config fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInstanceConfig {
    pub instance_name: String,
    pub channel_type: ChannelType,
    pub credentials: HashMap<String, String>,
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,
}

/// Status of a channel binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelBindingStatus {
    Active,
    Draining,
    Released,
}

/// Tracks a channel token bound to a specific agent instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelBinding {
    pub instance_id: String,
    pub channel_type: ChannelType,
    pub bot_token_hash: String,
    pub status: ChannelBindingStatus,
    pub bound_at_unix_ms: u64,
}

/// Connection status for a channel within a runtime instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelConnectionStatus {
    Connected,
    Disconnected,
    RateLimited,
    Proxied,
}

// ---------------------------------------------------------------------------
// Runtime metadata
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeMetadata {
    pub runtime: ClawRuntime,
    pub version: String,
    pub language: String,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub default_port: Option<u16>,
    #[serde(default)]
    pub config_format: Option<String>,
    #[serde(default)]
    pub channel_support: HashMap<ChannelType, ChannelSupport>,
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
    #[serde(default)]
    pub env_vars: Vec<(String, String)>,
    #[serde(default)]
    pub channels: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
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
