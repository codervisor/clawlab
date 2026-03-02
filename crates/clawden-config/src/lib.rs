use clawden_core::ClawRuntime;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;

// ---------------------------------------------------------------------------
// clawden.yaml schema (spec 017)
// ---------------------------------------------------------------------------

/// Top-level `clawden.yaml` config. Supports two forms:
///
/// **Single-runtime shorthand**:
/// ```yaml
/// runtime: zeroclaw
/// channels:
///   telegram:
///     token: $TELEGRAM_BOT_TOKEN
/// ```
///
/// **Multi-runtime full form**:
/// ```yaml
/// channels:
///   support-tg:
///     type: telegram
///     token: $SUPPORT_TG_TOKEN
/// runtimes:
///   - name: zeroclaw
///     channels: [support-tg]
///     tools: [git, http]
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClawDenYaml {
    /// Single-runtime shorthand (mutually exclusive with `runtimes`).
    #[serde(default)]
    pub runtime: Option<String>,

    /// Named channel instances.
    #[serde(default)]
    pub channels: HashMap<String, ChannelInstanceYaml>,

    /// Named LLM provider definitions.
    #[serde(default)]
    pub providers: HashMap<String, ProviderEntryYaml>,

    /// Multi-runtime list.
    #[serde(default)]
    pub runtimes: Vec<RuntimeEntryYaml>,

    /// Single-runtime tools shorthand.
    #[serde(default)]
    pub tools: Vec<String>,

    /// Single-runtime config overrides shorthand.
    #[serde(default)]
    pub config: HashMap<String, Value>,

    /// Single-runtime provider shorthand.
    #[serde(default)]
    pub provider: Option<ProviderRefYaml>,

    /// Single-runtime model shorthand.
    #[serde(default)]
    pub model: Option<String>,

    /// Single-runtime version constraint shorthand.
    #[serde(default)]
    pub version: Option<String>,
}

/// A channel instance entry in `clawden.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInstanceYaml {
    /// Channel platform type. Inferred from the key name if it matches a known type.
    #[serde(rename = "type", default)]
    pub channel_type: Option<String>,

    /// Bot/API token (supports `$ENV_VAR` syntax).
    #[serde(default)]
    pub token: Option<String>,

    /// Slack bot token.
    #[serde(default)]
    pub bot_token: Option<String>,

    /// Slack app token.
    #[serde(default)]
    pub app_token: Option<String>,

    /// Phone number for Signal.
    #[serde(default)]
    pub phone: Option<String>,

    /// Optional guild ID for Discord.
    #[serde(default)]
    pub guild: Option<String>,

    /// Optional user allowlist.
    #[serde(default)]
    pub allowed_users: Vec<String>,

    /// Optional role allowlist (Discord).
    #[serde(default)]
    pub allowed_roles: Vec<String>,

    /// Optional channel allowlist (Slack).
    #[serde(default)]
    pub allowed_channels: Vec<String>,

    /// Group activation mode.
    #[serde(default)]
    pub group_mode: Option<String>,

    /// Catch-all for any extra channel-specific fields.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// A runtime entry in the `runtimes` array.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEntryYaml {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub channels: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub config: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEntryYaml {
    #[serde(rename = "type", default)]
    pub provider_type: Option<LlmProvider>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub org_id: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

impl ProviderEntryYaml {
    fn resolved_type(&self, provider_name: &str) -> Option<LlmProvider> {
        self.provider_type
            .clone()
            .or_else(|| LlmProvider::from_name(provider_name))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProviderRefYaml {
    Name(String),
    Inline(ProviderEntryYaml),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    OpenAi,
    Anthropic,
    Google,
    Mistral,
    Groq,
    OpenRouter,
    Ollama,
    Custom(String),
}

impl LlmProvider {
    fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "openai" => Some(Self::OpenAi),
            "anthropic" => Some(Self::Anthropic),
            "google" => Some(Self::Google),
            "mistral" => Some(Self::Mistral),
            "groq" => Some(Self::Groq),
            "openrouter" => Some(Self::OpenRouter),
            "ollama" => Some(Self::Ollama),
            _ => None,
        }
    }

    fn default_base_url(&self) -> Option<&'static str> {
        match self {
            Self::OpenAi => Some("https://api.openai.com/v1"),
            Self::Anthropic => Some("https://api.anthropic.com"),
            Self::Google => Some("https://generativelanguage.googleapis.com"),
            Self::Mistral => Some("https://api.mistral.ai/v1"),
            Self::Groq => Some("https://api.groq.com/openai/v1"),
            Self::OpenRouter => Some("https://openrouter.ai/api/v1"),
            Self::Ollama => Some("http://localhost:11434/v1"),
            Self::Custom(_) => None,
        }
    }

    fn default_api_key_env(&self) -> Option<&'static str> {
        match self {
            Self::OpenAi => Some("OPENAI_API_KEY"),
            Self::Anthropic => Some("ANTHROPIC_API_KEY"),
            Self::Google => Some("GEMINI_API_KEY"),
            Self::Mistral => Some("MISTRAL_API_KEY"),
            Self::Groq => Some("GROQ_API_KEY"),
            Self::OpenRouter => Some("OPENROUTER_API_KEY"),
            Self::Ollama | Self::Custom(_) => None,
        }
    }

    fn resolve_api_key_from_env(&self) -> Option<String> {
        match self {
            // Match SDK behavior: GOOGLE_API_KEY takes precedence over GEMINI_API_KEY.
            Self::Google => std::env::var("GOOGLE_API_KEY")
                .ok()
                .or_else(|| std::env::var("GEMINI_API_KEY").ok()),
            _ => self
                .default_api_key_env()
                .and_then(|env| std::env::var(env).ok()),
        }
    }
}

/// Known built-in tools.
pub const KNOWN_TOOLS: &[&str] = &[
    "git",
    "http",
    "core-utils",
    "python",
    "code-tools",
    "database",
    "network",
    "sandbox",
    "browser",
    "gui",
    "compiler",
];

/// Known channel type names for type inference.
const KNOWN_CHANNEL_TYPES: &[&str] = &[
    "telegram",
    "discord",
    "slack",
    "whatsapp",
    "signal",
    "matrix",
    "email",
    "feishu",
    "lark",
    "dingtalk",
    "mattermost",
    "irc",
    "teams",
    "imessage",
    "google_chat",
    "qq",
    "line",
    "nostr",
];

impl ClawDenYaml {
    /// Parse a clawden.yaml file from disk. Auto-loads `.env` from the same directory.
    pub fn from_file(path: &Path) -> Result<Self, String> {
        // Auto-load .env from the directory containing clawden.yaml
        if let Some(dir) = path.parent() {
            let env_path = dir.join(".env");
            if env_path.exists() {
                let _ = dotenvy::from_path(&env_path);
            }
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        Self::parse_yaml(&content)
    }

    /// Parse from a YAML string.
    pub fn parse_yaml(yaml: &str) -> Result<Self, String> {
        serde_yaml::from_str(yaml).map_err(|e| format!("invalid clawden.yaml: {e}"))
    }

    /// Validate the config and return structured errors.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Must have either `runtime` or `runtimes`, not both
        if self.runtime.is_some() && !self.runtimes.is_empty() {
            errors.push(
                "cannot use both 'runtime' (shorthand) and 'runtimes' (multi) at the same time"
                    .to_string(),
            );
        }
        if self.runtime.is_none() && self.runtimes.is_empty() {
            errors.push("must specify either 'runtime' or 'runtimes'".to_string());
        }

        // Validate channel types can be resolved
        for (name, ch) in &self.channels {
            let resolved = ch.channel_type.as_deref().or_else(|| {
                if KNOWN_CHANNEL_TYPES.contains(&name.as_str()) {
                    Some(name.as_str())
                } else {
                    None
                }
            });
            if resolved.is_none() {
                errors.push(format!(
                    "Channel '{}' has no 'type' field and '{}' is not a known channel type. \
                     Add 'type: telegram' (or another supported type) to the channel config.",
                    name, name
                ));
            }
        }

        // Validate channel references exist and enforce 1:1 instance→runtime
        let mut channel_owners: HashMap<String, String> = HashMap::new();
        for rt in &self.runtimes {
            for ch_name in &rt.channels {
                if !self.channels.contains_key(ch_name) {
                    errors.push(format!(
                        "Runtime '{}' references channel '{}' which is not defined in 'channels:'.",
                        rt.name, ch_name
                    ));
                }
                if let Some(prev_owner) = channel_owners.get(ch_name) {
                    errors.push(format!(
                        "Channel '{}' is assigned to both '{}' and '{}'. \
                         Each channel instance can only connect to one runtime.",
                        ch_name, prev_owner, rt.name
                    ));
                } else {
                    channel_owners.insert(ch_name.clone(), rt.name.clone());
                }
            }
        }

        // Enforce token uniqueness per channel type to avoid bot conflicts.
        // We use `token` where available and fall back to `bot_token`.
        let mut seen_tokens: HashMap<(String, String), String> = HashMap::new();
        for (name, ch) in &self.channels {
            let Some(channel_type) = Self::resolve_channel_type(name, ch) else {
                continue;
            };
            let token = ch
                .token
                .clone()
                .or_else(|| ch.bot_token.clone())
                .unwrap_or_default();
            if token.is_empty() {
                continue;
            }
            let key = (channel_type, token.clone());
            if let Some(other_name) = seen_tokens.get(&key) {
                errors.push(format!(
                    "Channels '{}' and '{}' resolve to the same {} token. \
                     Each bot token can only be used by one channel instance.",
                    other_name, name, key.0
                ));
            } else {
                seen_tokens.insert(key, name.clone());
            }
        }

        for (provider_name, provider) in &self.providers {
            let resolved_type = provider.resolved_type(provider_name);
            if resolved_type.is_none() {
                errors.push(format!(
                    "Provider '{}' has no 'type' and is not a known provider name",
                    provider_name
                ));
            } else if matches!(resolved_type, Some(LlmProvider::Custom(_)))
                && provider.base_url.as_deref().is_none_or(str::is_empty)
            {
                errors.push(format!(
                    "Provider '{}' of type custom requires a non-empty 'base_url'",
                    provider_name
                ));
            }
        }

        for rt in &self.runtimes {
            if let Some(provider_name) = &rt.provider {
                let unknown = !self.providers.contains_key(provider_name)
                    && LlmProvider::from_name(provider_name).is_none();
                if unknown {
                    errors.push(format!(
                        "Runtime '{}' references provider '{}' which is not defined in 'providers:' and is not a known shorthand provider",
                        rt.name, provider_name
                    ));
                }
            }

            if let Some(version) = rt.version.as_deref() {
                if !valid_version_constraint(version) {
                    errors.push(format!(
                        "Runtime '{}' has invalid version constraint '{}'",
                        rt.name, version
                    ));
                }
            }
        }

        if let Some(version) = self.version.as_deref() {
            if !valid_version_constraint(version) {
                errors.push(format!(
                    "Top-level 'version' has invalid constraint '{}'",
                    version
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Resolve `$ENV_VAR` references in all credential fields.
    pub fn resolve_env_vars(&mut self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        for (name, ch) in &mut self.channels {
            resolve_field(&mut ch.token, "Channel", name, "token", &mut errors);
            resolve_field(&mut ch.bot_token, "Channel", name, "bot_token", &mut errors);
            resolve_field(&mut ch.app_token, "Channel", name, "app_token", &mut errors);
            resolve_field(&mut ch.phone, "Channel", name, "phone", &mut errors);
            resolve_field(&mut ch.guild, "Channel", name, "guild", &mut errors);
        }
        for (name, provider) in &mut self.providers {
            resolve_field(
                &mut provider.api_key,
                "Provider",
                name,
                "api_key",
                &mut errors,
            );
            resolve_field(
                &mut provider.base_url,
                "Provider",
                name,
                "base_url",
                &mut errors,
            );

            if let Some(provider_type) = provider.resolved_type(name) {
                if provider.api_key.is_none() {
                    provider.api_key = provider_type.resolve_api_key_from_env();
                }
                if provider.base_url.is_none() {
                    provider.base_url = provider_type.default_base_url().map(str::to_string);
                }
            }
        }
        if let Some(provider_ref) = &mut self.provider {
            let provider_map = self.providers.clone();
            let mut resolved_provider = match provider_ref {
                ProviderRefYaml::Inline(provider) => provider.clone(),
                ProviderRefYaml::Name(name) => {
                    provider_map
                        .get(name)
                        .cloned()
                        .unwrap_or(ProviderEntryYaml {
                            provider_type: LlmProvider::from_name(name),
                            api_key: None,
                            base_url: None,
                            org_id: None,
                            extra: HashMap::new(),
                        })
                }
            };

            resolve_field(
                &mut resolved_provider.api_key,
                "Provider",
                "provider",
                "api_key",
                &mut errors,
            );
            resolve_field(
                &mut resolved_provider.base_url,
                "Provider",
                "provider",
                "base_url",
                &mut errors,
            );

            let provider_name = match provider_ref {
                ProviderRefYaml::Name(name) => name.as_str(),
                ProviderRefYaml::Inline(_) => "provider",
            };
            if let Some(provider_type) = resolved_provider.resolved_type(provider_name) {
                if resolved_provider.api_key.is_none() {
                    resolved_provider.api_key = provider_type.resolve_api_key_from_env();
                }
                if resolved_provider.base_url.is_none() {
                    resolved_provider.base_url =
                        provider_type.default_base_url().map(str::to_string);
                }
            }
            *provider_ref = ProviderRefYaml::Inline(resolved_provider);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Resolve the channel type for a given instance name.
    pub fn resolve_channel_type(name: &str, ch: &ChannelInstanceYaml) -> Option<String> {
        ch.channel_type.clone().or_else(|| {
            if KNOWN_CHANNEL_TYPES.contains(&name) {
                Some(name.to_string())
            } else {
                None
            }
        })
    }
}

fn valid_version_constraint(raw: &str) -> bool {
    let value = raw.trim();
    if value.is_empty() {
        return false;
    }
    if value.eq_ignore_ascii_case("latest") {
        return true;
    }

    if let Some(prefix) = value
        .strip_suffix(".x")
        .or_else(|| value.strip_suffix(".*"))
    {
        let mut parts = prefix.split('.').filter(|p| !p.is_empty());
        let Some(major) = parts.next() else {
            return false;
        };
        let Some(minor) = parts.next() else {
            return false;
        };
        if parts.next().is_some() {
            return false;
        }
        return major.parse::<u64>().is_ok() && minor.parse::<u64>().is_ok();
    }

    if value.starts_with('>') || value.starts_with('<') || value.starts_with('=') {
        return VersionReq::parse(value).is_ok();
    }

    Version::parse(value.trim_start_matches('v')).is_ok()
}

/// Resolve a single `$ENV_VAR` field in-place.
fn resolve_field(
    field: &mut Option<String>,
    kind: &str,
    instance: &str,
    field_name: &str,
    errors: &mut Vec<String>,
) {
    if let Some(val) = field.as_ref() {
        if let Some(env_name) = val.strip_prefix('$') {
            match std::env::var(env_name) {
                Ok(resolved) => *field = Some(resolved),
                Err(_) => errors.push(format!(
                    "{} '{}' field '{}': environment variable '{}' is not set",
                    kind, instance, field_name, env_name
                )),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Canonical config types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClawDenConfig {
    pub agent: AgentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub runtime: ClawRuntime,
    pub model: ModelConfig,
    #[serde(default)]
    pub tools: Vec<ToolConfig>,
    #[serde(default)]
    pub channels: Vec<ChannelConfig>,
    pub security: SecurityConfig,
    #[serde(default)]
    pub extras: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub name: String,
    pub api_key_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    pub name: String,
    #[serde(default)]
    pub allowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    pub channel: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default)]
    pub allowlist: Vec<String>,
    #[serde(default)]
    pub sandboxed: bool,
}

impl ClawDenConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.agent.name.trim().is_empty() {
            return Err("agent.name must not be empty".to_string());
        }

        if self.agent.model.provider.trim().is_empty() || self.agent.model.name.trim().is_empty() {
            return Err("agent.model provider and name must not be empty".to_string());
        }

        Ok(())
    }

    pub fn to_safe_json(&self) -> Value {
        let mut value = serde_json::to_value(self).unwrap_or(Value::Null);
        if let Some(api_ref) = value
            .get_mut("agent")
            .and_then(|a| a.get_mut("model"))
            .and_then(|m| m.get_mut("api_key_ref"))
        {
            *api_ref = Value::String("<redacted>".to_string());
        }
        value
    }
}

pub trait RuntimeConfigTranslator {
    fn runtime(&self) -> ClawRuntime;
    fn to_runtime_config(&self, canonical: &ClawDenConfig) -> Result<Value, String>;
    #[allow(clippy::wrong_self_convention)]
    fn from_runtime_config(&self, runtime_config: &Value) -> Result<ClawDenConfig, String>;
}

pub struct OpenClawConfigTranslator;
pub struct ZeroClawConfigTranslator;
pub struct PicoClawConfigTranslator;
pub struct NanoClawConfigTranslator;

impl RuntimeConfigTranslator for OpenClawConfigTranslator {
    fn runtime(&self) -> ClawRuntime {
        ClawRuntime::OpenClaw
    }

    fn to_runtime_config(&self, canonical: &ClawDenConfig) -> Result<Value, String> {
        canonical.validate()?;
        Ok(serde_json::json!({
            "runtime": "openclaw",
            "agent": canonical.agent.name,
            "model": canonical.agent.model.name,
            "provider": canonical.agent.model.provider,
            "apiKeyRef": canonical.agent.model.api_key_ref,
            "tools": canonical.agent.tools,
            "channels": canonical.agent.channels,
            "security": canonical.agent.security,
            "extras": canonical.agent.extras,
        }))
    }

    fn from_runtime_config(&self, runtime_config: &Value) -> Result<ClawDenConfig, String> {
        let agent = runtime_config
            .get("agent")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing openclaw agent field".to_string())?;
        let model = runtime_config
            .get("model")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing openclaw model field".to_string())?;
        let provider = runtime_config
            .get("provider")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing openclaw provider field".to_string())?;

        let mut config = base_config_with_runtime(
            agent,
            ClawRuntime::OpenClaw,
            provider,
            model,
            runtime_config,
        );
        config.agent.model.api_key_ref = runtime_config
            .get("apiKeyRef")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        Ok(config)
    }
}

impl RuntimeConfigTranslator for ZeroClawConfigTranslator {
    fn runtime(&self) -> ClawRuntime {
        ClawRuntime::ZeroClaw
    }

    fn to_runtime_config(&self, canonical: &ClawDenConfig) -> Result<Value, String> {
        canonical.validate()?;
        Ok(serde_json::json!({
            "runtime": "zeroclaw",
            "agent": {
                "name": canonical.agent.name,
                "model": canonical.agent.model,
                "tools": canonical.agent.tools,
                "channels": canonical.agent.channels,
                "security": canonical.agent.security,
            },
            "extras": canonical.agent.extras,
        }))
    }

    fn from_runtime_config(&self, runtime_config: &Value) -> Result<ClawDenConfig, String> {
        let agent_obj = runtime_config
            .get("agent")
            .and_then(Value::as_object)
            .ok_or_else(|| "missing zeroclaw agent object".to_string())?;
        let name = agent_obj
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing zeroclaw agent.name".to_string())?;
        let model: ModelConfig = serde_json::from_value(
            agent_obj
                .get("model")
                .cloned()
                .ok_or_else(|| "missing zeroclaw agent.model".to_string())?,
        )
        .map_err(|err| format!("invalid zeroclaw model: {err}"))?;

        let mut config = base_config_with_runtime(
            name,
            ClawRuntime::ZeroClaw,
            &model.provider,
            &model.name,
            runtime_config,
        );
        config.agent.model.api_key_ref = model.api_key_ref;
        Ok(config)
    }
}

impl RuntimeConfigTranslator for PicoClawConfigTranslator {
    fn runtime(&self) -> ClawRuntime {
        ClawRuntime::PicoClaw
    }

    fn to_runtime_config(&self, canonical: &ClawDenConfig) -> Result<Value, String> {
        canonical.validate()?;
        Ok(serde_json::json!({
            "runtime": "picoclaw",
            "name": canonical.agent.name,
            "llm": {
                "provider": canonical.agent.model.provider,
                "model": canonical.agent.model.name,
                "apiKeyRef": canonical.agent.model.api_key_ref,
            },
            "tools": canonical.agent.tools,
            "channels": canonical.agent.channels,
            "policy": canonical.agent.security,
            "extras": canonical.agent.extras,
        }))
    }

    fn from_runtime_config(&self, runtime_config: &Value) -> Result<ClawDenConfig, String> {
        let name = runtime_config
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing picoclaw name".to_string())?;
        let llm = runtime_config
            .get("llm")
            .ok_or_else(|| "missing picoclaw llm object".to_string())?;

        let provider = llm
            .get("provider")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing picoclaw llm.provider".to_string())?;
        let model = llm
            .get("model")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing picoclaw llm.model".to_string())?;

        let mut config =
            base_config_with_runtime(name, ClawRuntime::PicoClaw, provider, model, runtime_config);
        config.agent.model.api_key_ref = llm
            .get("apiKeyRef")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        Ok(config)
    }
}

impl RuntimeConfigTranslator for NanoClawConfigTranslator {
    fn runtime(&self) -> ClawRuntime {
        ClawRuntime::NanoClaw
    }

    fn to_runtime_config(&self, canonical: &ClawDenConfig) -> Result<Value, String> {
        canonical.validate()?;
        Ok(serde_json::json!({
            "runtime": "nanoclaw",
            "agent": {
                "name": canonical.agent.name,
                "provider": canonical.agent.model.provider,
                "model": canonical.agent.model.name,
                "apiKeyRef": canonical.agent.model.api_key_ref,
            },
            "tools": canonical.agent.tools,
            "channels": canonical.agent.channels,
            "security": canonical.agent.security,
            "extras": canonical.agent.extras,
        }))
    }

    fn from_runtime_config(&self, runtime_config: &Value) -> Result<ClawDenConfig, String> {
        let agent_obj = runtime_config
            .get("agent")
            .and_then(Value::as_object)
            .ok_or_else(|| "missing nanoclaw agent object".to_string())?;
        let name = agent_obj
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing nanoclaw agent.name".to_string())?;
        let provider = agent_obj
            .get("provider")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing nanoclaw agent.provider".to_string())?;
        let model = agent_obj
            .get("model")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing nanoclaw agent.model".to_string())?;

        let mut config =
            base_config_with_runtime(name, ClawRuntime::NanoClaw, provider, model, runtime_config);
        config.agent.model.api_key_ref = agent_obj
            .get("apiKeyRef")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        Ok(config)
    }
}

fn base_config_with_runtime(
    name: &str,
    runtime: ClawRuntime,
    provider: &str,
    model: &str,
    runtime_config: &Value,
) -> ClawDenConfig {
    let tools = runtime_config
        .get("tools")
        .cloned()
        .or_else(|| {
            runtime_config
                .get("agent")
                .and_then(|agent| agent.get("tools"))
                .cloned()
        })
        .unwrap_or_else(|| Value::Array(vec![]));

    let channels = runtime_config
        .get("channels")
        .cloned()
        .or_else(|| {
            runtime_config
                .get("agent")
                .and_then(|agent| agent.get("channels"))
                .cloned()
        })
        .unwrap_or_else(|| Value::Array(vec![]));

    let security = runtime_config
        .get("security")
        .cloned()
        .or_else(|| {
            runtime_config.get("policy").cloned().or_else(|| {
                runtime_config
                    .get("agent")
                    .and_then(|agent| agent.get("security"))
                    .cloned()
            })
        })
        .unwrap_or_else(|| Value::Object(Map::new()));

    let extras = runtime_config
        .get("extras")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    ClawDenConfig {
        agent: AgentConfig {
            name: name.to_string(),
            runtime,
            model: ModelConfig {
                provider: provider.to_string(),
                name: model.to_string(),
                api_key_ref: None,
            },
            tools: serde_json::from_value(tools).unwrap_or_default(),
            channels: serde_json::from_value(channels).unwrap_or_default(),
            security: serde_json::from_value(security).unwrap_or(SecurityConfig {
                allowlist: vec![],
                sandboxed: false,
            }),
            extras,
        },
    }
}

// ---------------------------------------------------------------------------
// Secret Vault — encrypted at-rest secret store
// ---------------------------------------------------------------------------

/// A simple XOR-based obfuscation key for the in-memory vault.
/// In production, this would delegate to age/sops or a system keychain;
/// here we provide the API surface with a basic symmetric cipher to protect
/// secrets at rest in memory dumps.
pub struct SecretVault {
    /// Secrets stored as (name → encrypted_bytes).
    store: HashMap<String, Vec<u8>>,
    /// Symmetric key for XOR obfuscation. In production, use a real KDF + AES.
    key: Vec<u8>,
}

impl SecretVault {
    /// Create a new vault with the given encryption key.
    pub fn new(key: &[u8]) -> Self {
        assert!(!key.is_empty(), "vault key must not be empty");
        Self {
            store: HashMap::new(),
            key: key.to_vec(),
        }
    }

    /// Store a secret. The value is encrypted before being stored.
    pub fn put(&mut self, name: &str, plaintext: &str) {
        let encrypted = Self::xor_bytes(plaintext.as_bytes(), &self.key);
        self.store.insert(name.to_string(), encrypted);
    }

    /// Retrieve and decrypt a secret by name. Returns `None` if not found.
    pub fn get(&self, name: &str) -> Option<String> {
        self.store.get(name).map(|encrypted| {
            let decrypted = Self::xor_bytes(encrypted, &self.key);
            String::from_utf8_lossy(&decrypted).into_owned()
        })
    }

    /// Remove a secret.
    pub fn remove(&mut self, name: &str) -> bool {
        self.store.remove(name).is_some()
    }

    /// List all secret names (values are never exposed).
    pub fn list_names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.store.keys().cloned().collect();
        names.sort();
        names
    }

    /// Export encrypted entries so callers can persist them to disk without
    /// exposing plaintext values.
    pub fn export_encrypted_hex(&self) -> HashMap<String, String> {
        self.store
            .iter()
            .map(|(name, bytes)| (name.clone(), hex_encode(bytes)))
            .collect()
    }

    /// Rebuild a vault from previously exported encrypted entries.
    pub fn from_encrypted_hex(
        key: &[u8],
        entries: &HashMap<String, String>,
    ) -> Result<Self, String> {
        if key.is_empty() {
            return Err("vault key must not be empty".to_string());
        }

        let mut store = HashMap::new();
        for (name, value) in entries {
            let decoded = hex_decode(value)?;
            store.insert(name.clone(), decoded);
        }

        Ok(Self {
            store,
            key: key.to_vec(),
        })
    }

    /// Resolve all `api_key_ref` values in a config by injecting from the vault.
    /// Returns a new config with the `api_key_ref` field replaced by the actual
    /// secret value. This is intended for deploy-time injection only; the result
    /// should never be logged or persisted.
    pub fn resolve_config(&self, config: &ClawDenConfig) -> Result<ClawDenConfig, String> {
        let mut resolved = config.clone();
        if let Some(ref key_ref) = resolved.agent.model.api_key_ref {
            let secret = self
                .get(key_ref)
                .ok_or_else(|| format!("secret '{}' not found in vault", key_ref))?;
            resolved.agent.model.api_key_ref = Some(secret);
        }
        Ok(resolved)
    }

    fn xor_bytes(data: &[u8], key: &[u8]) -> Vec<u8> {
        data.iter()
            .enumerate()
            .map(|(i, byte)| byte ^ key[i % key.len()])
            .collect()
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn hex_decode(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err("invalid hex length".to_string());
    }
    let mut out = Vec::with_capacity(value.len() / 2);
    for idx in (0..value.len()).step_by(2) {
        let part = &value[idx..idx + 2];
        let byte = u8::from_str_radix(part, 16).map_err(|_| "invalid hex byte".to_string())?;
        out.push(byte);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Config diff & drift detection
// ---------------------------------------------------------------------------

/// A single difference between two configs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigDiff {
    pub path: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

/// Compare two configs and return the list of differences.
pub fn diff_configs(expected: &ClawDenConfig, actual: &ClawDenConfig) -> Vec<ConfigDiff> {
    let expected_json = serde_json::to_value(expected).unwrap_or(Value::Null);
    let actual_json = serde_json::to_value(actual).unwrap_or(Value::Null);
    let mut diffs = Vec::new();
    diff_value("", &expected_json, &actual_json, &mut diffs);
    diffs
}

fn diff_value(path: &str, expected: &Value, actual: &Value, diffs: &mut Vec<ConfigDiff>) {
    match (expected, actual) {
        (Value::Object(exp_map), Value::Object(act_map)) => {
            let all_keys: HashSet<_> = exp_map.keys().chain(act_map.keys()).collect();
            for key in all_keys {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                let exp_val = exp_map.get(key).unwrap_or(&Value::Null);
                let act_val = act_map.get(key).unwrap_or(&Value::Null);
                diff_value(&child_path, exp_val, act_val, diffs);
            }
        }
        (Value::Array(exp_arr), Value::Array(act_arr)) => {
            let max_len = exp_arr.len().max(act_arr.len());
            for i in 0..max_len {
                let child_path = format!("{path}[{i}]");
                let exp_val = exp_arr.get(i).unwrap_or(&Value::Null);
                let act_val = act_arr.get(i).unwrap_or(&Value::Null);
                diff_value(&child_path, exp_val, act_val, diffs);
            }
        }
        _ => {
            if expected != actual {
                diffs.push(ConfigDiff {
                    path: path.to_string(),
                    expected: Some(expected.to_string()),
                    actual: Some(actual.to_string()),
                });
            }
        }
    }
}

/// Detect drift: compare the canonical config against the runtime's current config.
/// Returns an empty vec if in sync.
pub fn detect_drift(
    translator: &dyn RuntimeConfigTranslator,
    canonical: &ClawDenConfig,
    runtime_native: &Value,
) -> Result<Vec<ConfigDiff>, String> {
    let actual_canonical = translator.from_runtime_config(runtime_native)?;
    Ok(diff_configs(canonical, &actual_canonical))
}

// ---------------------------------------------------------------------------
// Channel credential translation (spec 018)
// ---------------------------------------------------------------------------

/// Translates a clawden.yaml channel instance into the format a specific runtime expects.
pub struct ChannelCredentialMapper;

impl ChannelCredentialMapper {
    /// Generate OpenClaw JSON5 config fragment for a channel instance.
    /// OpenClaw uses per-channel library config (grammY, discord.js, Baileys, Bolt).
    pub fn openclaw_channel_config(
        channel_type: &str,
        ch: &ChannelInstanceYaml,
    ) -> Result<Value, String> {
        match channel_type {
            "telegram" => Ok(serde_json::json!({
                "telegram": { "token": ch.token.as_deref().unwrap_or("") }
            })),
            "discord" => {
                let mut cfg = serde_json::json!({
                    "discord": { "token": ch.token.as_deref().unwrap_or("") }
                });
                if let Some(guild) = &ch.guild {
                    cfg["discord"]["guild"] = Value::String(guild.clone());
                }
                Ok(cfg)
            }
            "slack" => Ok(serde_json::json!({
                "slack": {
                    "botToken": ch.bot_token.as_deref().unwrap_or(""),
                    "appToken": ch.app_token.as_deref().unwrap_or("")
                }
            })),
            // WhatsApp: Baileys driver uses a phone number; Meta Cloud API uses a bearer token.
            "whatsapp" => {
                if let Some(phone) = &ch.phone {
                    Ok(serde_json::json!({
                        "whatsapp": { "driver": "baileys", "phone": phone }
                    }))
                } else {
                    Ok(serde_json::json!({
                        "whatsapp": { "driver": "cloud-api", "token": ch.token.as_deref().unwrap_or("") }
                    }))
                }
            }
            "signal" => Ok(serde_json::json!({
                "signal": {
                    "phone": ch.phone.as_deref().unwrap_or(""),
                    "token": ch.token.as_deref().unwrap_or("")
                }
            })),
            "feishu" | "lark" => Ok(serde_json::json!({
                "feishu": { "token": ch.token.as_deref().unwrap_or("") }
            })),
            _ => Ok(serde_json::json!({
                channel_type: { "token": ch.token.as_deref().unwrap_or("") }
            })),
        }
    }

    /// Generate ZeroClaw env vars for a channel instance.
    /// ZeroClaw uses `ZEROCLAW_<CHANNEL>_<FIELD>` prefixed env vars.
    pub fn zeroclaw_env_vars(
        channel_type: &str,
        ch: &ChannelInstanceYaml,
    ) -> HashMap<String, String> {
        let prefix = format!("ZEROCLAW_{}", channel_type.to_uppercase());
        let mut vars = HashMap::new();
        match channel_type {
            "signal" => {
                if let Some(phone) = &ch.phone {
                    vars.insert(format!("{prefix}_PHONE"), phone.clone());
                }
                if let Some(token) = &ch.token {
                    vars.insert(format!("{prefix}_TOKEN"), token.clone());
                }
            }
            "slack" => {
                // Slack requires both bot token (RTM/Events) and app token (Socket Mode).
                if let Some(bt) = &ch.bot_token {
                    vars.insert(format!("{prefix}_BOT_TOKEN"), bt.clone());
                }
                if let Some(at) = &ch.app_token {
                    vars.insert(format!("{prefix}_APP_TOKEN"), at.clone());
                }
            }
            "whatsapp" => {
                // WhatsApp: Baileys uses phone number; Meta Cloud API uses bearer token.
                if let Some(phone) = &ch.phone {
                    vars.insert(format!("{prefix}_PHONE"), phone.clone());
                    vars.insert(format!("{prefix}_DRIVER"), "baileys".to_string());
                } else if let Some(token) = &ch.token {
                    vars.insert(format!("{prefix}_TOKEN"), token.clone());
                    vars.insert(format!("{prefix}_DRIVER"), "cloud-api".to_string());
                }
            }
            _ => {
                if let Some(token) = &ch.token {
                    vars.insert(format!("{prefix}_BOT_TOKEN"), token.clone());
                }
                if let Some(phone) = &ch.phone {
                    vars.insert(format!("{prefix}_PHONE"), phone.clone());
                }
            }
        }
        vars
    }

    /// Generate NanoClaw env vars for a channel instance.
    /// NanoClaw uses `NANOCLAW_<CHANNEL>_<FIELD>` prefixed env vars.
    pub fn nanoclaw_env_vars(
        channel_type: &str,
        ch: &ChannelInstanceYaml,
    ) -> HashMap<String, String> {
        let prefix = format!("NANOCLAW_{}", channel_type.to_uppercase());
        let mut vars = HashMap::new();
        match channel_type {
            "slack" => {
                // Slack requires both bot token and app token.
                if let Some(bt) = &ch.bot_token {
                    vars.insert(format!("{prefix}_BOT_TOKEN"), bt.clone());
                }
                if let Some(at) = &ch.app_token {
                    vars.insert(format!("{prefix}_APP_TOKEN"), at.clone());
                }
            }
            "whatsapp" => {
                // WhatsApp: Baileys uses phone number; Meta Cloud API uses bearer token.
                if let Some(phone) = &ch.phone {
                    vars.insert(format!("{prefix}_PHONE"), phone.clone());
                    vars.insert(format!("{prefix}_DRIVER"), "baileys".to_string());
                } else if let Some(token) = &ch.token {
                    vars.insert(format!("{prefix}_TOKEN"), token.clone());
                    vars.insert(format!("{prefix}_DRIVER"), "cloud-api".to_string());
                }
            }
            _ => {
                if let Some(token) = &ch.token {
                    vars.insert(format!("{prefix}_TOKEN"), token.clone());
                }
                if let Some(bt) = &ch.bot_token {
                    vars.insert(format!("{prefix}_BOT_TOKEN"), bt.clone());
                }
                if let Some(at) = &ch.app_token {
                    vars.insert(format!("{prefix}_APP_TOKEN"), at.clone());
                }
            }
        }
        vars
    }

    /// Generate PicoClaw JSON config fragment for a channel instance.
    /// PicoClaw uses `config.<channel>.<field>` in JSON.
    pub fn picoclaw_channel_config(
        channel_type: &str,
        ch: &ChannelInstanceYaml,
    ) -> Result<Value, String> {
        let mut cfg = serde_json::Map::new();
        if let Some(token) = &ch.token {
            cfg.insert("token".to_string(), Value::String(token.clone()));
        }
        if let Some(bt) = &ch.bot_token {
            cfg.insert("bot_token".to_string(), Value::String(bt.clone()));
        }
        if let Some(at) = &ch.app_token {
            cfg.insert("app_token".to_string(), Value::String(at.clone()));
        }
        // WhatsApp (Baileys): include phone number and driver hint.
        if channel_type == "whatsapp" {
            if let Some(phone) = &ch.phone {
                cfg.insert("phone".to_string(), Value::String(phone.clone()));
                cfg.insert("driver".to_string(), Value::String("baileys".to_string()));
            } else if !cfg.contains_key("token") {
                // no credentials at all — mark driver but leave empty
                cfg.insert("driver".to_string(), Value::String("cloud-api".to_string()));
            }
        }

        if channel_type == "dingtalk" {
            if let Some(app_id) = ch.extra.get("app_id").and_then(Value::as_str) {
                cfg.insert("app_id".to_string(), Value::String(app_id.to_string()));
            }
            if let Some(app_secret) = ch.extra.get("app_secret").and_then(Value::as_str) {
                cfg.insert(
                    "app_secret".to_string(),
                    Value::String(app_secret.to_string()),
                );
            }
        }

        if channel_type == "qq" {
            if let Some(uin) = ch.extra.get("uin").and_then(Value::as_str) {
                cfg.insert("uin".to_string(), Value::String(uin.to_string()));
            }
        }

        Ok(serde_json::json!({ channel_type: cfg }))
    }

    /// Generate IronClaw WASM capability config for a channel instance.
    /// IronClaw channels are represented as capability descriptors.
    pub fn ironclaw_channel_config(
        channel_type: &str,
        ch: &ChannelInstanceYaml,
    ) -> Result<Value, String> {
        let mut capability = serde_json::Map::new();
        capability.insert("type".to_string(), Value::String(channel_type.to_string()));

        if let Some(token) = &ch.token {
            capability.insert("token".to_string(), Value::String(token.clone()));
        }
        if let Some(bt) = &ch.bot_token {
            capability.insert("bot_token".to_string(), Value::String(bt.clone()));
        }
        if let Some(at) = &ch.app_token {
            capability.insert("app_token".to_string(), Value::String(at.clone()));
        }
        if let Some(phone) = &ch.phone {
            capability.insert("phone".to_string(), Value::String(phone.clone()));
        }
        for (k, v) in &ch.extra {
            capability.insert(k.clone(), v.clone());
        }

        Ok(serde_json::json!({
            "wasm_capabilities": [capability]
        }))
    }

    /// Generate NullClaw JSON config fragment for a channel instance.
    pub fn nullclaw_channel_config(
        channel_type: &str,
        ch: &ChannelInstanceYaml,
    ) -> Result<Value, String> {
        let mut cfg = serde_json::Map::new();
        if let Some(token) = &ch.token {
            cfg.insert("token".to_string(), Value::String(token.clone()));
        }
        if let Some(phone) = &ch.phone {
            cfg.insert("phone".to_string(), Value::String(phone.clone()));
        }
        for (k, v) in &ch.extra {
            cfg.insert(k.clone(), v.clone());
        }
        Ok(serde_json::json!({ channel_type: cfg }))
    }

    /// Generate MicroClaw YAML-like map fragment for a channel instance.
    /// We return JSON values because the config layer normalizes to `serde_json::Value`.
    pub fn microclaw_channel_config(
        channel_type: &str,
        ch: &ChannelInstanceYaml,
    ) -> Result<Value, String> {
        let mut cfg = serde_json::Map::new();
        if let Some(token) = &ch.token {
            cfg.insert("token".to_string(), Value::String(token.clone()));
        }
        if let Some(bt) = &ch.bot_token {
            cfg.insert("bot_token".to_string(), Value::String(bt.clone()));
        }
        if let Some(at) = &ch.app_token {
            cfg.insert("app_token".to_string(), Value::String(at.clone()));
        }
        if let Some(phone) = &ch.phone {
            cfg.insert("phone".to_string(), Value::String(phone.clone()));
        }
        for (k, v) in &ch.extra {
            cfg.insert(k.clone(), v.clone());
        }
        Ok(serde_json::json!({ channel_type: cfg }))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        diff_configs, ChannelCredentialMapper, ChannelInstanceYaml, ClawDenConfig, ClawDenYaml,
        LlmProvider, ModelConfig, NanoClawConfigTranslator, OpenClawConfigTranslator,
        PicoClawConfigTranslator, ProviderRefYaml, RuntimeConfigTranslator, SecretVault,
        ZeroClawConfigTranslator,
    };
    use crate::{AgentConfig, ChannelConfig, SecurityConfig, ToolConfig};
    use clawden_core::ClawRuntime;
    use serde_json::Map;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn sample_channel() -> ChannelInstanceYaml {
        ChannelInstanceYaml {
            channel_type: None,
            token: None,
            bot_token: None,
            app_token: None,
            phone: None,
            guild: None,
            allowed_users: vec![],
            allowed_roles: vec![],
            allowed_channels: vec![],
            group_mode: None,
            extra: std::collections::HashMap::new(),
        }
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("clawden-config-{name}-{stamp}"));
        fs::create_dir_all(&path).expect("temp dir should be created");
        path
    }

    fn sample_config(runtime: ClawRuntime) -> ClawDenConfig {
        ClawDenConfig {
            agent: AgentConfig {
                name: "alpha".to_string(),
                runtime,
                model: ModelConfig {
                    provider: "openai".to_string(),
                    name: "gpt-5-mini".to_string(),
                    api_key_ref: Some("secret/openai".to_string()),
                },
                tools: vec![ToolConfig {
                    name: "web_search".to_string(),
                    allowed: true,
                }],
                channels: vec![ChannelConfig {
                    channel: "telegram".to_string(),
                    enabled: true,
                }],
                security: SecurityConfig {
                    allowlist: vec!["team".to_string()],
                    sandboxed: true,
                },
                extras: Map::new(),
            },
        }
    }

    #[test]
    fn openclaw_roundtrip_preserves_core_fields() {
        let translator = OpenClawConfigTranslator;
        let canonical = sample_config(ClawRuntime::OpenClaw);
        let native = translator
            .to_runtime_config(&canonical)
            .expect("openclaw to native should succeed");
        let decoded = translator
            .from_runtime_config(&native)
            .expect("openclaw from native should succeed");

        assert_eq!(decoded.agent.runtime, ClawRuntime::OpenClaw);
        assert_eq!(decoded.agent.name, "alpha");
        assert_eq!(decoded.agent.model.name, "gpt-5-mini");
        assert_eq!(
            decoded.agent.model.api_key_ref.as_deref(),
            Some("secret/openai")
        );
    }

    #[test]
    fn zeroclaw_roundtrip_preserves_core_fields() {
        let translator = ZeroClawConfigTranslator;
        let canonical = sample_config(ClawRuntime::ZeroClaw);
        let native = translator
            .to_runtime_config(&canonical)
            .expect("zeroclaw to native should succeed");
        let decoded = translator
            .from_runtime_config(&native)
            .expect("zeroclaw from native should succeed");

        assert_eq!(decoded.agent.runtime, ClawRuntime::ZeroClaw);
        assert_eq!(decoded.agent.name, "alpha");
        assert_eq!(decoded.agent.model.provider, "openai");
    }

    #[test]
    fn picoclaw_roundtrip_preserves_core_fields() {
        let translator = PicoClawConfigTranslator;
        let canonical = sample_config(ClawRuntime::PicoClaw);
        let native = translator
            .to_runtime_config(&canonical)
            .expect("picoclaw to native should succeed");
        let decoded = translator
            .from_runtime_config(&native)
            .expect("picoclaw from native should succeed");

        assert_eq!(decoded.agent.runtime, ClawRuntime::PicoClaw);
        assert_eq!(decoded.agent.name, "alpha");
        assert_eq!(
            decoded.agent.model.api_key_ref.as_deref(),
            Some("secret/openai")
        );
    }

    #[test]
    fn nanoclaw_roundtrip_preserves_core_fields() {
        let translator = NanoClawConfigTranslator;
        let canonical = sample_config(ClawRuntime::NanoClaw);
        let native = translator
            .to_runtime_config(&canonical)
            .expect("nanoclaw to native should succeed");
        let decoded = translator
            .from_runtime_config(&native)
            .expect("nanoclaw from native should succeed");

        assert_eq!(decoded.agent.runtime, ClawRuntime::NanoClaw);
        assert_eq!(decoded.agent.name, "alpha");
        assert_eq!(decoded.agent.model.provider, "openai");
        assert_eq!(decoded.agent.model.name, "gpt-5-mini");
    }

    #[test]
    fn secret_vault_encrypt_decrypt_roundtrip() {
        let mut vault = SecretVault::new(b"test-encryption-key");
        vault.put("secret/openai", "sk-abc123");

        assert_eq!(vault.get("secret/openai").as_deref(), Some("sk-abc123"));
        assert_eq!(vault.list_names(), vec!["secret/openai".to_string()]);
    }

    #[test]
    fn secret_vault_remove() {
        let mut vault = SecretVault::new(b"key");
        vault.put("api-key", "value");
        assert!(vault.remove("api-key"));
        assert!(vault.get("api-key").is_none());
    }

    #[test]
    fn secret_vault_resolve_config() {
        let mut vault = SecretVault::new(b"key");
        vault.put("secret/openai", "sk-real-key-123");

        let config = sample_config(ClawRuntime::OpenClaw);
        let resolved = vault.resolve_config(&config).unwrap();
        assert_eq!(
            resolved.agent.model.api_key_ref.as_deref(),
            Some("sk-real-key-123")
        );
    }

    #[test]
    fn secret_vault_export_import_roundtrip() {
        let mut vault = SecretVault::new(b"persist-key");
        vault.put("provider/openai", "sk-secret");

        let exported = vault.export_encrypted_hex();
        let reloaded = SecretVault::from_encrypted_hex(b"persist-key", &exported)
            .expect("vault should reload from encrypted map");

        assert_eq!(
            reloaded.get("provider/openai").as_deref(),
            Some("sk-secret")
        );
    }

    #[test]
    fn diff_configs_detects_name_change() {
        let config_a = sample_config(ClawRuntime::OpenClaw);
        let mut config_b = config_a.clone();
        config_b.agent.name = "beta".to_string();

        let diffs = diff_configs(&config_a, &config_b);
        assert!(!diffs.is_empty());
        assert!(diffs.iter().any(|d| d.path.contains("name")));
    }

    #[test]
    fn diff_configs_identical_returns_empty() {
        let config = sample_config(ClawRuntime::OpenClaw);
        let diffs = diff_configs(&config, &config);
        assert!(diffs.is_empty());
    }

    #[test]
    fn safe_json_redacts_api_key() {
        let config = sample_config(ClawRuntime::OpenClaw);
        let safe = config.to_safe_json();
        let api_ref = safe["agent"]["model"]["api_key_ref"].as_str().unwrap();
        assert_eq!(api_ref, "<redacted>");
    }

    #[test]
    fn yaml_parses_providers_and_runtime_references() {
        let yaml = r#"
runtime: zeroclaw
providers:
  openai:
    api_key: $OPENAI_API_KEY
runtimes:
  - name: zeroclaw
    provider: openai
    model: gpt-4o
"#;
        let parsed = ClawDenYaml::parse_yaml(yaml).expect("yaml should parse");
        assert!(parsed.providers.contains_key("openai"));
        assert_eq!(parsed.runtimes[0].provider.as_deref(), Some("openai"));
        assert_eq!(parsed.runtimes[0].model.as_deref(), Some("gpt-4o"));
    }

    #[test]
    fn provider_env_vars_and_defaults_resolve() {
        std::env::set_var("OPENAI_API_KEY", "sk-from-env");
        let yaml = r#"
runtime: zeroclaw
providers:
  openai: {}
"#;
        let mut parsed = ClawDenYaml::parse_yaml(yaml).expect("yaml should parse");
        parsed.resolve_env_vars().expect("env vars should resolve");
        let provider = parsed.providers.get("openai").expect("provider exists");
        assert_eq!(provider.api_key.as_deref(), Some("sk-from-env"));
        assert_eq!(
            provider.base_url.as_deref(),
            Some("https://api.openai.com/v1")
        );
    }

    #[test]
    fn single_runtime_provider_shorthand_resolves_defaults() {
        std::env::set_var("OPENAI_API_KEY", "sk-shorthand");
        let yaml = r#"
runtime: zeroclaw
provider: openai
model: gpt-4o-mini
"#;
        let mut parsed = ClawDenYaml::parse_yaml(yaml).expect("yaml should parse");
        parsed.resolve_env_vars().expect("env vars should resolve");

        let ProviderRefYaml::Inline(provider) = parsed.provider.expect("provider should exist")
        else {
            panic!("provider shorthand should be normalized to inline provider");
        };
        assert_eq!(provider.api_key.as_deref(), Some("sk-shorthand"));
        assert_eq!(
            provider.base_url.as_deref(),
            Some("https://api.openai.com/v1")
        );
    }

    #[test]
    fn google_provider_prefers_google_api_key_over_gemini() {
        std::env::set_var("GOOGLE_API_KEY", "google-key");
        std::env::set_var("GEMINI_API_KEY", "gemini-key");
        let yaml = r#"
runtime: zeroclaw
providers:
  google: {}
"#;
        let mut parsed = ClawDenYaml::parse_yaml(yaml).expect("yaml should parse");
        parsed.resolve_env_vars().expect("env vars should resolve");

        let provider = parsed
            .providers
            .get("google")
            .expect("provider should exist");
        assert_eq!(provider.api_key.as_deref(), Some("google-key"));
    }

    #[test]
    fn custom_provider_requires_base_url() {
        let mut parsed = ClawDenYaml::parse_yaml("runtime: zeroclaw").expect("yaml should parse");
        parsed.providers.insert(
            "local".to_string(),
            super::ProviderEntryYaml {
                provider_type: Some(LlmProvider::Custom("lm-studio".to_string())),
                api_key: None,
                base_url: None,
                org_id: None,
                extra: std::collections::HashMap::new(),
            },
        );
        let errors = parsed.validate().expect_err("validation should fail");
        assert!(errors
            .iter()
            .any(|e| e.contains("requires a non-empty 'base_url'")));
    }

    #[test]
    fn llm_provider_parses_known_type() {
        let provider: LlmProvider =
            serde_yaml::from_str("openai").expect("provider enum should parse");
        assert_eq!(provider, LlmProvider::OpenAi);
    }

    #[test]
    fn runtime_unknown_provider_reference_fails_validation() {
        let yaml = r#"
runtimes:
  - name: zeroclaw
    provider: not-a-real-provider
"#;
        let parsed = ClawDenYaml::parse_yaml(yaml).expect("yaml should parse");
        let errors = parsed.validate().expect_err("validation should fail");
        assert!(errors
            .iter()
            .any(|e| e.contains("references provider 'not-a-real-provider'")));
    }

    #[test]
    fn from_file_loads_dotenv_for_provider_keys() {
        let dir = temp_dir("dotenv-provider");
        fs::write(dir.join(".env"), "MISTRAL_API_KEY=sk-from-dotenv\n")
            .expect("dotenv file should be written");
        fs::write(
            dir.join("clawden.yaml"),
            "runtime: zeroclaw\nprovider: mistral\nmodel: mistral-small-latest\n",
        )
        .expect("yaml should be written");

        let mut parsed =
            ClawDenYaml::from_file(&dir.join("clawden.yaml")).expect("yaml should load from file");
        parsed.resolve_env_vars().expect("env vars should resolve");

        let ProviderRefYaml::Inline(provider) = parsed.provider.expect("provider should exist")
        else {
            panic!("provider shorthand should resolve into inline provider")
        };
        assert_eq!(provider.api_key.as_deref(), Some("sk-from-dotenv"));
    }

    #[test]
    fn invalid_provider_type_is_rejected_with_clear_error() {
        let err = ClawDenYaml::parse_yaml(
            "runtime: zeroclaw\nproviders:\n  openai:\n    type: open_ai\n",
        )
        .expect_err("invalid provider type should fail parsing");
        assert!(err.contains("unknown variant `open_ai`"));
        assert!(err.contains("openai"));
    }

    #[test]
    fn validation_fails_when_channel_type_is_missing_and_not_inferable() {
        let yaml = r#"
runtimes:
  - name: zeroclaw
    channels: [my-chat]
channels:
  my-chat:
    token: abc
"#;
        let parsed = ClawDenYaml::parse_yaml(yaml).expect("yaml should parse");
        let errors = parsed.validate().expect_err("validation should fail");
        assert!(errors
            .iter()
            .any(|e| e.contains("has no 'type' field") && e.contains("my-chat")));
    }

    #[test]
    fn validation_fails_when_runtime_references_undefined_channel() {
        let yaml = r#"
runtimes:
  - name: zeroclaw
    channels: [ghost]
channels:
  telegram:
    token: t1
"#;
        let parsed = ClawDenYaml::parse_yaml(yaml).expect("yaml should parse");
        let errors = parsed.validate().expect_err("validation should fail");
        assert!(errors
            .iter()
            .any(|e| e.contains("references channel 'ghost'")));
    }

    #[test]
    fn validation_fails_when_same_channel_instance_assigned_to_two_runtimes() {
        let yaml = r#"
runtimes:
  - name: zeroclaw
    channels: [support-tg]
  - name: picoclaw
    channels: [support-tg]
channels:
  support-tg:
    type: telegram
    token: t1
"#;
        let parsed = ClawDenYaml::parse_yaml(yaml).expect("yaml should parse");
        let errors = parsed.validate().expect_err("validation should fail");
        assert!(errors.iter().any(|e| e.contains("assigned to both")));
    }

    #[test]
    fn validation_fails_when_same_token_reused_for_same_channel_type() {
        let yaml = r#"
runtimes:
  - name: zeroclaw
    channels: [support-tg]
  - name: picoclaw
    channels: [creative-tg]
channels:
  support-tg:
    type: telegram
    token: same-token
  creative-tg:
    type: telegram
    token: same-token
"#;
        let parsed = ClawDenYaml::parse_yaml(yaml).expect("yaml should parse");
        let errors = parsed.validate().expect_err("validation should fail");
        assert!(errors
            .iter()
            .any(|e| e.contains("resolve to the same telegram token")));
    }

    #[test]
    fn channel_type_inference_works_for_known_instance_name() {
        let ch = ChannelInstanceYaml {
            channel_type: None,
            token: None,
            bot_token: None,
            app_token: None,
            phone: None,
            guild: None,
            allowed_users: vec![],
            allowed_roles: vec![],
            allowed_channels: vec![],
            group_mode: None,
            extra: std::collections::HashMap::new(),
        };
        assert_eq!(
            ClawDenYaml::resolve_channel_type("telegram", &ch).as_deref(),
            Some("telegram")
        );
    }

    #[test]
    fn channel_env_var_references_resolve() {
        std::env::set_var("TELEGRAM_BOT_TOKEN", "resolved-token");
        let yaml = r#"
runtimes:
  - name: zeroclaw
    channels: [telegram]
channels:
  telegram:
    token: $TELEGRAM_BOT_TOKEN
"#;
        let mut parsed = ClawDenYaml::parse_yaml(yaml).expect("yaml should parse");
        parsed.resolve_env_vars().expect("env vars should resolve");
        let token = parsed
            .channels
            .get("telegram")
            .and_then(|c| c.token.as_deref());
        assert_eq!(token, Some("resolved-token"));
    }

    #[test]
    fn multiple_telegram_instances_with_unique_tokens_validate() {
        let yaml = r#"
runtimes:
  - name: zeroclaw
    channels: [support-tg]
  - name: picoclaw
    channels: [creative-tg]
channels:
  support-tg:
    type: telegram
    token: tg-a
  creative-tg:
    type: telegram
    token: tg-b
"#;
        let parsed = ClawDenYaml::parse_yaml(yaml).expect("yaml should parse");
        parsed.validate().expect("validation should pass");
    }

    #[test]
    fn validation_rejects_invalid_top_level_version_constraint() {
        let yaml = r#"
runtime: zeroclaw
version: "this-is-not-semver"
channels:
  telegram:
    token: "abc"
"#;
        let parsed = ClawDenYaml::parse_yaml(yaml).expect("yaml should parse");
        let errors = parsed.validate().expect_err("validation should fail");
        assert!(
            errors.iter().any(|e| e.contains("Top-level 'version'")),
            "expected top-level version validation error, got: {errors:?}"
        );
    }

    #[test]
    fn validation_rejects_invalid_runtime_version_constraint() {
        let yaml = r#"
channels:
  support:
    type: telegram
    token: "abc"
runtimes:
  - name: zeroclaw
    version: ">=>1.2.3"
    channels: [support]
"#;
        let parsed = ClawDenYaml::parse_yaml(yaml).expect("yaml should parse");
        let errors = parsed.validate().expect_err("validation should fail");
        assert!(
            errors
                .iter()
                .any(|e| e.contains("Runtime 'zeroclaw' has invalid version constraint")),
            "expected runtime version validation error, got: {errors:?}"
        );
    }

    #[test]
    fn zeroclaw_signal_mapping_uses_phone_and_token() {
        let mut ch = sample_channel();
        ch.phone = Some("+123456789".to_string());
        ch.token = Some("sig-token".to_string());
        let vars = ChannelCredentialMapper::zeroclaw_env_vars("signal", &ch);
        assert_eq!(
            vars.get("ZEROCLAW_SIGNAL_PHONE").map(String::as_str),
            Some("+123456789")
        );
        assert_eq!(
            vars.get("ZEROCLAW_SIGNAL_TOKEN").map(String::as_str),
            Some("sig-token")
        );
    }

    #[test]
    fn openclaw_signal_mapping_contains_signal_block() {
        let mut ch = sample_channel();
        ch.phone = Some("+15551234567".to_string());
        let cfg = ChannelCredentialMapper::openclaw_channel_config("signal", &ch)
            .expect("signal config should map");
        assert_eq!(
            cfg.get("signal")
                .and_then(|v| v.get("phone"))
                .and_then(|v| v.as_str()),
            Some("+15551234567")
        );
    }

    #[test]
    fn picoclaw_dingtalk_mapping_uses_extra_fields() {
        let mut ch = sample_channel();
        ch.extra
            .insert("app_id".to_string(), serde_json::json!("dt-app"));
        ch.extra
            .insert("app_secret".to_string(), serde_json::json!("dt-secret"));
        let cfg = ChannelCredentialMapper::picoclaw_channel_config("dingtalk", &ch)
            .expect("dingtalk config should map");
        assert_eq!(
            cfg.get("dingtalk")
                .and_then(|v| v.get("app_id"))
                .and_then(|v| v.as_str()),
            Some("dt-app")
        );
        assert_eq!(
            cfg.get("dingtalk")
                .and_then(|v| v.get("app_secret"))
                .and_then(|v| v.as_str()),
            Some("dt-secret")
        );
    }

    #[test]
    fn picoclaw_qq_mapping_uses_uin() {
        let mut ch = sample_channel();
        ch.extra
            .insert("uin".to_string(), serde_json::json!("10001"));
        let cfg = ChannelCredentialMapper::picoclaw_channel_config("qq", &ch)
            .expect("qq config should map");
        assert_eq!(
            cfg.get("qq")
                .and_then(|v| v.get("uin"))
                .and_then(|v| v.as_str()),
            Some("10001")
        );
    }

    #[test]
    fn ironclaw_mapping_uses_wasm_capability_shape() {
        let mut ch = sample_channel();
        ch.token = Some("iron-token".to_string());
        let cfg = ChannelCredentialMapper::ironclaw_channel_config("telegram", &ch)
            .expect("ironclaw config should map");
        assert_eq!(
            cfg.get("wasm_capabilities")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.get("type"))
                .and_then(|v| v.as_str()),
            Some("telegram")
        );
    }

    #[test]
    fn nullclaw_mapping_includes_extra_fields() {
        let mut ch = sample_channel();
        ch.token = Some("null-token".to_string());
        ch.extra
            .insert("endpoint".to_string(), serde_json::json!("https://hook"));
        let cfg = ChannelCredentialMapper::nullclaw_channel_config("matrix", &ch)
            .expect("nullclaw config should map");
        assert_eq!(
            cfg.get("matrix")
                .and_then(|v| v.get("endpoint"))
                .and_then(|v| v.as_str()),
            Some("https://hook")
        );
    }

    #[test]
    fn microclaw_mapping_includes_phone_and_token() {
        let mut ch = sample_channel();
        ch.token = Some("micro-token".to_string());
        ch.phone = Some("+18885550123".to_string());
        let cfg = ChannelCredentialMapper::microclaw_channel_config("signal", &ch)
            .expect("microclaw config should map");
        assert_eq!(
            cfg.get("signal")
                .and_then(|v| v.get("token"))
                .and_then(|v| v.as_str()),
            Some("micro-token")
        );
        assert_eq!(
            cfg.get("signal")
                .and_then(|v| v.get("phone"))
                .and_then(|v| v.as_str()),
            Some("+18885550123")
        );
    }
}
