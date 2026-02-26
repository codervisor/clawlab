use clawden_core::ClawRuntime;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

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

        Ok(base_config_with_runtime(
            agent,
            ClawRuntime::OpenClaw,
            provider,
            model,
            runtime_config,
        ))
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

#[cfg(test)]
mod tests {
    use super::{
        diff_configs, ClawDenConfig, ModelConfig, OpenClawConfigTranslator,
        PicoClawConfigTranslator, RuntimeConfigTranslator, SecretVault, ZeroClawConfigTranslator,
    };
    use crate::{AgentConfig, ChannelConfig, SecurityConfig, ToolConfig};
    use clawden_core::ClawRuntime;
    use serde_json::Map;

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
}
