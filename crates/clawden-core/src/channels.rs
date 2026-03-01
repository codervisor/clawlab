use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    ChannelBinding, ChannelBindingStatus, ChannelConnectionStatus, ChannelInstanceConfig,
    ChannelType,
};

#[derive(Default)]
pub struct ChannelStore {
    configs: HashMap<String, ChannelInstanceConfig>,
    bindings: HashMap<(String, String), ChannelBinding>,
    next_binding_id: u64,
    assignments: HashMap<String, Vec<String>>,
    connection_status: HashMap<(String, String), ChannelConnectionStatus>,
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
