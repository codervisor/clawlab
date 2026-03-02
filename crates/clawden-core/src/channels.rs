use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    AgentState, ChannelBinding, ChannelBindingStatus, ChannelConnectionStatus,
    ChannelInstanceConfig, ChannelType,
};

/// Health report entry for a single channel instance.
#[derive(Debug, Clone, Serialize)]
pub struct ChannelHealthEntry {
    pub instance_name: String,
    pub channel_type: String,
    pub agent_id: Option<String>,
    pub status: ChannelConnectionStatus,
    pub last_checked_unix_ms: Option<u64>,
}

#[derive(Default)]
pub struct ChannelStore {
    configs: HashMap<String, ChannelInstanceConfig>,
    bindings: HashMap<(String, String), ChannelBinding>,
    next_binding_id: u64,
    assignments: HashMap<String, Vec<String>>,
    connection_status: HashMap<(String, String), ChannelConnectionStatus>,
    /// Unix ms timestamp of the last channel health refresh.
    last_health_check_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BindingConflict {
    pub channel_type: String,
    pub bot_token_hash: String,
    pub instance_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChannelTypeSummary {
    pub channel_type: String,
    pub instance_count: usize,
    pub connected: usize,
    pub disconnected: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelCredentialCheck {
    pub instance_name: String,
    pub channel_type: String,
    pub ok: bool,
    #[serde(default)]
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelConfigRequest {
    pub instance_name: String,
    pub channel_type: String,
    #[serde(default)]
    pub credentials: HashMap<String, String>,
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BindChannelRequest {
    pub instance_id: String,
    pub channel_type: String,
    pub bot_token: String,
}

impl ChannelStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert_config(
        &mut self,
        req: ChannelConfigRequest,
    ) -> Result<ChannelInstanceConfig, String> {
        let channel_type = ChannelType::from_str_loose(&req.channel_type)
            .ok_or_else(|| format!("unknown channel type: {}", req.channel_type))?;

        let config = ChannelInstanceConfig {
            instance_name: req.instance_name.clone(),
            channel_type,
            credentials: req.credentials,
            options: req.options,
        };
        self.configs.insert(req.instance_name, config.clone());
        Ok(config)
    }

    pub fn delete_config(&mut self, instance_name: &str) -> bool {
        self.configs.remove(instance_name).is_some()
    }

    pub fn list_configs_by_type(&self, channel_type: &ChannelType) -> Vec<&ChannelInstanceConfig> {
        self.configs
            .values()
            .filter(|c| &c.channel_type == channel_type)
            .collect()
    }

    pub fn validate_channel_type_credentials(
        &self,
        channel_type: &ChannelType,
    ) -> Vec<ChannelCredentialCheck> {
        self.list_configs_by_type(channel_type)
            .into_iter()
            .map(Self::validate_channel_config)
            .collect()
    }

    pub fn validate_channel_config(config: &ChannelInstanceConfig) -> ChannelCredentialCheck {
        let mut errors = Vec::new();
        let get = |k: &str| {
            config
                .credentials
                .get(k)
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
        };

        match config.channel_type {
            ChannelType::Telegram | ChannelType::Discord | ChannelType::Feishu => {
                if get("token").is_none() {
                    errors.push("missing required credential: token".to_string());
                }
            }
            ChannelType::Slack => {
                if get("bot_token").is_none() {
                    errors.push("missing required credential: bot_token".to_string());
                }
                if get("app_token").is_none() {
                    errors.push("missing required credential: app_token".to_string());
                }
            }
            ChannelType::Whatsapp => {
                if get("token").is_none() && get("phone").is_none() {
                    errors.push("missing required credential: token or phone".to_string());
                }
            }
            ChannelType::Signal => {
                if get("phone").is_none() {
                    errors.push("missing required credential: phone".to_string());
                }
            }
            ChannelType::Dingtalk => {
                if get("app_id").is_none() {
                    errors.push("missing required credential: app_id".to_string());
                }
                if get("app_secret").is_none() {
                    errors.push("missing required credential: app_secret".to_string());
                }
            }
            ChannelType::Qq => {
                if get("uin").is_none() && get("token").is_none() {
                    errors.push("missing required credential: uin or token".to_string());
                }
            }
            _ => {
                if get("token").is_none() {
                    errors.push("missing required credential: token".to_string());
                }
            }
        }

        ChannelCredentialCheck {
            instance_name: config.instance_name.clone(),
            channel_type: config.channel_type.to_string(),
            ok: errors.is_empty(),
            errors,
        }
    }

    /// Allowlist model:
    /// - [] or missing: deny all
    /// - ["*"]: allow all
    /// - otherwise: exact match only
    pub fn authorize_sender_for_channel(
        &self,
        instance_name: &str,
        sender_id: &str,
        sender_role: Option<&str>,
    ) -> Result<bool, String> {
        let config = self
            .configs
            .get(instance_name)
            .ok_or_else(|| format!("channel instance '{instance_name}' not found"))?;

        let users = list_option_strings(&config.options, "allowed_users");
        let roles = list_option_strings(&config.options, "allowed_roles");

        if users.is_empty() && roles.is_empty() {
            return Ok(false);
        }
        if users.iter().any(|v| v == "*") || roles.iter().any(|v| v == "*") {
            return Ok(true);
        }
        if users.iter().any(|v| v == sender_id) {
            return Ok(true);
        }
        if let Some(role) = sender_role {
            if roles.iter().any(|v| v == role) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn list_channel_summaries(&self) -> Vec<ChannelTypeSummary> {
        let mut type_map: HashMap<String, (usize, usize, usize)> = HashMap::new();
        for config in self.configs.values() {
            let key = config.channel_type.to_string();
            let entry = type_map.entry(key).or_insert((0, 0, 0));
            entry.0 += 1;
        }

        for ((_, channel_name), status) in &self.connection_status {
            if let Some(config) = self.configs.get(channel_name) {
                let key = config.channel_type.to_string();
                if let Some(entry) = type_map.get_mut(&key) {
                    match status {
                        ChannelConnectionStatus::Connected | ChannelConnectionStatus::Proxied => {
                            entry.1 += 1;
                        }
                        _ => {
                            entry.2 += 1;
                        }
                    }
                }
            }
        }

        type_map
            .into_iter()
            .map(
                |(channel_type, (instance_count, connected, disconnected))| ChannelTypeSummary {
                    channel_type,
                    instance_count,
                    connected,
                    disconnected,
                },
            )
            .collect()
    }

    pub fn bind(
        &mut self,
        instance_id: String,
        channel_type: &str,
        bot_token: &str,
    ) -> Result<ChannelBinding, String> {
        let ct = ChannelType::from_str_loose(channel_type)
            .ok_or_else(|| format!("unknown channel type: {channel_type}"))?;
        let token_hash = hash_token(bot_token);
        let key = (ct.to_string(), token_hash.clone());

        if let Some(existing) = self.bindings.get(&key) {
            if existing.status == ChannelBindingStatus::Active
                && existing.instance_id != instance_id
            {
                return Err(format!(
                    "token already bound to instance {}",
                    existing.instance_id
                ));
            }
        }

        let binding = ChannelBinding {
            instance_id,
            channel_type: ct,
            bot_token_hash: token_hash,
            status: ChannelBindingStatus::Active,
            bound_at_unix_ms: current_unix_ms(),
        };
        self.bindings.insert(key, binding.clone());
        self.next_binding_id += 1;
        Ok(binding)
    }

    pub fn unbind(&mut self, binding_id: usize) -> Result<ChannelBinding, String> {
        let keys: Vec<_> = self.bindings.keys().cloned().collect();
        let key = keys
            .get(binding_id)
            .ok_or_else(|| format!("binding {binding_id} not found"))?
            .clone();
        if let Some(binding) = self.bindings.get_mut(&key) {
            binding.status = ChannelBindingStatus::Released;
            Ok(binding.clone())
        } else {
            Err(format!("binding {binding_id} not found"))
        }
    }

    pub fn list_bindings(&self) -> Vec<ChannelBinding> {
        self.bindings.values().cloned().collect()
    }

    pub fn detect_conflicts(&self) -> Vec<BindingConflict> {
        let mut groups: HashMap<(String, String), Vec<String>> = HashMap::new();
        for binding in self.bindings.values() {
            if binding.status == ChannelBindingStatus::Active {
                let key = (
                    binding.channel_type.to_string(),
                    binding.bot_token_hash.clone(),
                );
                groups
                    .entry(key)
                    .or_default()
                    .push(binding.instance_id.clone());
            }
        }

        groups
            .into_iter()
            .filter(|(_, ids)| ids.len() > 1)
            .map(
                |((channel_type, bot_token_hash), instance_ids)| BindingConflict {
                    channel_type,
                    bot_token_hash,
                    instance_ids,
                },
            )
            .collect()
    }

    pub fn assign_channel(&mut self, agent_id: &str, channel_instance_name: &str) {
        let list = self.assignments.entry(agent_id.to_string()).or_default();
        if !list.contains(&channel_instance_name.to_string()) {
            list.push(channel_instance_name.to_string());
        }
    }

    pub fn get_agent_channels(&self, agent_id: &str) -> Vec<&ChannelInstanceConfig> {
        self.assignments
            .get(agent_id)
            .map(|names| names.iter().filter_map(|n| self.configs.get(n)).collect())
            .unwrap_or_default()
    }

    pub fn get_connection_status(
        &self,
        agent_id: &str,
        channel_name: &str,
    ) -> ChannelConnectionStatus {
        self.connection_status
            .get(&(agent_id.to_string(), channel_name.to_string()))
            .cloned()
            .unwrap_or(ChannelConnectionStatus::Disconnected)
    }

    /// Set the connection status for a channel instance assigned to an agent.
    pub fn set_connection_status(
        &mut self,
        agent_id: &str,
        channel_name: &str,
        status: ChannelConnectionStatus,
    ) {
        self.connection_status
            .insert((agent_id.to_string(), channel_name.to_string()), status);
    }

    /// Refresh channel health based on current agent states.
    ///
    /// For each `(agent_id, channel_instance)` assignment:
    /// - Agent is `Running` and channel is natively supported → `Connected`
    /// - Agent is `Running` but channel is proxied → `Proxied`
    /// - Agent is in any other state → `Disconnected`
    ///
    /// `proxy_pairs` is the set of `(agent_id, instance_name)` pairs that
    /// should be marked `Proxied` rather than `Connected`. The caller
    /// populates this using adapter metadata from `LifecycleManager`.
    pub fn refresh_channel_health(
        &mut self,
        agent_states: &HashMap<String, AgentState>,
        proxy_pairs: &std::collections::HashSet<(String, String)>,
    ) {
        self.last_health_check_unix_ms = Some(current_unix_ms());
        for (agent_id, instance_names) in &self.assignments {
            let running = matches!(agent_states.get(agent_id), Some(AgentState::Running));
            for instance_name in instance_names {
                let status = if running {
                    if proxy_pairs.contains(&(agent_id.clone(), instance_name.clone())) {
                        ChannelConnectionStatus::Proxied
                    } else {
                        ChannelConnectionStatus::Connected
                    }
                } else {
                    ChannelConnectionStatus::Disconnected
                };
                self.connection_status
                    .insert((agent_id.clone(), instance_name.clone()), status);
            }
        }
    }

    /// Return a per-instance health report across all configured channels.
    pub fn channel_health_report(&self) -> Vec<ChannelHealthEntry> {
        // Build a reverse lookup: instance_name → (agent_id, status)
        let mut instance_agent: HashMap<String, (String, ChannelConnectionStatus)> = HashMap::new();
        for ((agent_id, instance_name), status) in &self.connection_status {
            instance_agent
                .entry(instance_name.clone())
                .or_insert_with(|| (agent_id.clone(), status.clone()));
        }

        let mut report: Vec<ChannelHealthEntry> = self
            .configs
            .values()
            .map(|config| {
                let (agent_id, status) = instance_agent
                    .get(&config.instance_name)
                    .map(|(aid, s)| (Some(aid.clone()), s.clone()))
                    .unwrap_or((None, ChannelConnectionStatus::Disconnected));
                ChannelHealthEntry {
                    instance_name: config.instance_name.clone(),
                    channel_type: config.channel_type.to_string(),
                    agent_id,
                    status,
                    last_checked_unix_ms: self.last_health_check_unix_ms,
                }
            })
            .collect();

        // Sort for deterministic output
        report.sort_by(|a, b| a.instance_name.cmp(&b.instance_name));
        report
    }

    pub fn build_matrix(&self, agents: &[(String, String)]) -> Vec<MatrixRow> {
        let mut rows = Vec::new();
        for config in self.configs.values() {
            let mut statuses = Vec::new();
            for (agent_id, runtime) in agents {
                let status = self.get_connection_status(agent_id, &config.instance_name);
                statuses.push(MatrixCell {
                    agent_id: agent_id.clone(),
                    runtime: runtime.clone(),
                    status,
                });
            }
            rows.push(MatrixRow {
                channel_instance: config.instance_name.clone(),
                channel_type: config.channel_type.to_string(),
                cells: statuses,
            });
        }
        rows
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MatrixRow {
    pub channel_instance: String,
    pub channel_type: String,
    pub cells: Vec<MatrixCell>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MatrixCell {
    pub agent_id: String,
    pub runtime: String,
    pub status: ChannelConnectionStatus,
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn current_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_millis() as u64
}

fn list_option_strings(options: &HashMap<String, serde_json::Value>, key: &str) -> Vec<String> {
    options
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(serde_json::Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChannelType;

    fn config(instance_name: &str, channel_type: ChannelType) -> ChannelInstanceConfig {
        ChannelInstanceConfig {
            instance_name: instance_name.to_string(),
            channel_type,
            credentials: HashMap::new(),
            options: HashMap::new(),
        }
    }

    #[test]
    fn validate_slack_requires_dual_tokens() {
        let mut cfg = config("slack-main", ChannelType::Slack);
        cfg.credentials
            .insert("bot_token".to_string(), "xoxb-1".to_string());
        let check = ChannelStore::validate_channel_config(&cfg);
        assert!(!check.ok);
        assert!(check
            .errors
            .iter()
            .any(|e| e.contains("missing required credential: app_token")));
    }

    #[test]
    fn authorize_sender_allowlist_rules() {
        let mut store = ChannelStore::new();
        let mut cfg = config("tg-main", ChannelType::Telegram);
        cfg.options.insert(
            "allowed_users".to_string(),
            serde_json::json!(["123", "456"]),
        );
        store.configs.insert(cfg.instance_name.clone(), cfg);

        assert!(store
            .authorize_sender_for_channel("tg-main", "123", None)
            .expect("authorization should work"));
        assert!(!store
            .authorize_sender_for_channel("tg-main", "999", None)
            .expect("authorization should work"));
    }

    #[test]
    fn authorize_sender_wildcard_allows_all() {
        let mut store = ChannelStore::new();
        let mut cfg = config("discord-main", ChannelType::Discord);
        cfg.options
            .insert("allowed_roles".to_string(), serde_json::json!(["*"]));
        store.configs.insert(cfg.instance_name.clone(), cfg);

        assert!(store
            .authorize_sender_for_channel("discord-main", "u1", Some("member"))
            .expect("authorization should work"));
    }
}
