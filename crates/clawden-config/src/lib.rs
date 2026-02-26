use clawden_core::ClawRuntime;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

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

        let mut config = base_config_with_runtime(
            name,
            ClawRuntime::PicoClaw,
            provider,
            model,
            runtime_config,
        );
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
            runtime_config
                .get("policy")
                .cloned()
                .or_else(|| {
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

#[cfg(test)]
mod tests {
    use super::{
        ClawDenConfig, ModelConfig, OpenClawConfigTranslator, PicoClawConfigTranslator,
        RuntimeConfigTranslator, ZeroClawConfigTranslator,
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
        assert_eq!(decoded.agent.model.api_key_ref.as_deref(), Some("secret/openai"));
    }
}
